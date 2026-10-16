import {itemPathRibbon, pathRibbon, type PathStyle} from "../tools/paths";
import type {AssetLookup, Item, Point, Project} from "../map/model";
import {bounds, paintOrder, prefabParts} from "../map/model";
import {roofGeometry, type RoofKind} from "../tools/roofs";
import {previewText} from "./textPreview";
import {caveFootprint, outsideCaveMasks} from "../tools/caves";
import {mainLevelIndex} from "../map/levels";
import {terrainContours} from "../tools/terrain";
import {
    closedRing,
    type GeometryMode,
    geometryRings,
    type GeometryTarget,
    midpointHandles,
    type Vertex,
    vertexHandles
} from "../tools/architecture";

export type TextureUpload = {
    key: string;
    width: number;
    height: number;
    data: string;
    format: string;
    repeat: boolean;
    premultiplied: boolean;
    alias: string | null;
};
export type GpuMesh = {
    key: string;
    item?: string | null;
    texture: string;
    mode: number;
    clip: number[];
    vertices: string;
    indices: string;
    vertex_count: number;
    index_count: number;
};
export type GpuScene = {
    underlay_meshes?: number;
    text_bounds?: Record<string, import("../tools/text").TextBounds>;
    generation: string;
    revision: number;
    width: number;
    height: number;
    meshes: GpuMesh[];
    textures: TextureUpload[];
    used_textures: string[];
    stats: {
        build_ms: number;
        decoded_images: number;
        cache_hits: number;
        vertices: number;
        texture_bytes: number;
    };
};
type Resource = TextureUpload & {
    image: Promise<ImageBitmap> | null;
    raw: Uint8Array | null;
    touched: number;
};
const resources = new Map<string, Resource>();
const meshResources = new Map<string, { vertices: Float32Array; indices: Uint32Array; touched: number }>();
const meshBudget = 32 * 1024 * 1024;
let pinnedMeshes = new Set<string>();
let tick = 0;
let pinned = new Set<string>();
const budget = 192 * 1024 * 1024;

export function decodeBase64(value: string) {
    const binary = atob(value),
        bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    return bytes;
}

export function knownGpuTextures() {
    return [...resources.keys()];
}

export function knownGpuMeshes() {
    return [...meshResources.keys()];
}

export function ingestGpuScene(scene: GpuScene) {
    for (const m of scene.meshes) {
        if (!meshResources.has(m.key) && m.vertices) {
            meshResources.set(m.key, {
                vertices: new Float32Array(decodeBase64(m.vertices).buffer),
                indices: new Uint32Array(decodeBase64(m.indices).buffer),
                touched: ++tick,
            });
        }
    }
    pruneMeshes(new Set([...pinnedMeshes, ...scene.meshes.map((m) => m.key)]));
    for (const t of scene.textures) {
        if (resources.has(t.key)) continue;
        const bytes = t.alias ? null : decodeBase64(t.data);
        const image =
            bytes && t.format !== "rgba"
                ? createImageBitmap(
                    new Blob([bytes.buffer as ArrayBuffer], {type: t.format}),
                    {premultiplyAlpha: "none", imageOrientation: "none", colorSpaceConversion: "none"},
                )
                : null;
        image?.catch(() => {
        });
        resources.set(t.key, {
            ...t,
            data: "",
            image,
            raw: t.format === "rgba" ? bytes : null,
            touched: ++tick,
        });
    }
    pruneResources(scene.used_textures);
}

export function retainGpuResources(keys: string[], meshKeys: string[] = []) {
    pinned = new Set(keys);
    pinnedMeshes = new Set(meshKeys);
    pruneMeshes(pinnedMeshes);
    pruneResources();
}

function pruneMeshes(keep: Set<string>) {
    let size = [...meshResources.values()].reduce((n, m) => n + m.vertices.byteLength + m.indices.byteLength, 0);
    for (const [key, m] of [...meshResources].sort((a, b) => a[1].touched - b[1].touched)) {
        if (size <= meshBudget) break;
        if (keep.has(key)) continue;
        meshResources.delete(key);
        size -= m.vertices.byteLength + m.indices.byteLength;
    }
}

function pruneResources(incoming: string[] = []) {
    const protectedKeys = new Set([...pinned, ...incoming]);
    let size = 0;
    for (const r of resources.values())
        if (!r.alias) size += r.width * r.height * 4;
    for (const [key, r] of [...resources].sort(
        (a, b) => a[1].touched - b[1].touched,
    )) {
        if (size <= budget) break;
        if (protectedKeys.has(key)) continue;
        resources.delete(key);
        if (!r.alias) size -= r.width * r.height * 4;
        r.image?.then(
            (image) => image.close(),
            () => {
            },
        );
    }
}

const vertexShader = `#version 300 es
precision highp float;
layout(location=0) in vec2 a_position;
layout(location=1) in vec2 a_uv;
layout(location=2) in vec4 a_color;
layout(location=3) in vec4 a_tint;
uniform vec2 u_viewport; uniform vec2 u_origin; uniform float u_scale;
out vec2 v_uv;out vec4 v_color;out vec4 v_tint;
void main(){vec2 p=(a_position*u_scale+u_origin)/u_viewport;gl_Position=vec4(p.x*2.-1.,1.-p.y*2.,0.,1.);v_uv=a_uv;v_color=a_color;v_tint=a_tint;}`;
const fragmentShader = `#version 300 es
precision highp float;
uniform sampler2D u_texture;uniform int u_mode;uniform int u_premultiplied;
uniform vec2 u_blur_step;uniform float u_dim;
uniform vec4 u_pick;
in vec2 v_uv;in vec4 v_color;in vec4 v_tint;out vec4 out_color;
void main(){vec4 s=texture(u_texture,v_uv);
 if(u_pick.a>0.){float alpha=s.a*(u_mode==1?v_tint.a:v_color.a);if(alpha<=0.)discard;out_color=u_pick;return;}
 if(u_mode==2){vec4 sum=vec4(0.);float total=0.;for(int i=-4;i<=4;i++){float w=exp(-float(i*i)/8.);sum+=texture(u_texture,v_uv+float(i)*u_blur_step)*w;total+=w;}out_color=sum/total;out_color.rgb*=u_dim;return;}
 if(u_mode==1){float dominant=max(0.,s.r-max(s.g,s.b));float range=max(0.,s.r-min(s.g,s.b));float hue=range>0.?60.*abs(s.g-s.b)/range:60.;float mask=dominant*clamp((30.-hue)/10.,0.,1.);vec3 rgb=vec3(s.r-mask,s.g,s.b)+mask*v_tint.rgb;float alpha=s.a*v_tint.a;out_color=vec4(rgb*v_color.rgb*alpha,alpha);}
 else {if(u_premultiplied==0)s.rgb*=s.a;out_color=s*v_color;}}
`;
type Draw = {
    texture: string;
    mode: number;
    clip: number[];
    vao: WebGLVertexArrayObject;
    vbo: WebGLBuffer;
    ebo: WebGLBuffer;
    count: number;
    bounds: [number, number, number, number];
};
type Resident = {
    texture: WebGLTexture;
    bytes: number;
    touched: number;
    premultiplied: boolean;
};
export type GpuView = {
    origin: Point;
    scale: number;
    width: number;
    height: number;
};
export type Overlay = {
    caveBlasts?: Point[][];
    items: Item[];
    assets?: AssetLookup;
    gesture: Point[];
    drag: { mode: string; start: Point; last: Point } | null;
    radius: number;
    brushShape?: "circle" | "square" | "polygon";
    brushCursor?: Point | null;
    accent: number[];
    geometryMode?: GeometryMode | null;
    geometryHover?: GeometryTarget | null;
    vertex?: Vertex | null;
    drawingContour?: Point[];
    roof?: Pick<RoofKind, "points" | "width" | "roof_type">;
    path?: { points: Point[]; width: number; style: PathStyle };
};

export class GpuRenderer {
    readonly gl: WebGL2RenderingContext;
    readonly name: string;
    private program: WebGLProgram;
    private draws: Draw[] = [];
    private meshBuffers = new Map<string, { draw: Draw; bytes: number; touched: number }>();
    private textures = new Map<string, Resident>();
    private frame = 0;
    private scene: GpuScene | null = null;
    private overlay: Draw;
    private white: WebGLTexture;
    private blurQuad: Draw;
    private targets: { texture: WebGLTexture; framebuffer: WebGLFramebuffer }[] = [];
    private targetSize = "";
    private pickTarget: { texture: WebGLTexture; framebuffer: WebGLFramebuffer } | null = null;
    private uniforms: Record<string, WebGLUniformLocation | null> = {};
    private disposed = false;
    private token = 0;

    constructor(readonly canvas: HTMLCanvasElement) {
        const gl = canvas.getContext("webgl2", {
            alpha: true,
            antialias: true,
            premultipliedAlpha: true,
            failIfMajorPerformanceCaveat: true,
        });
        if (!gl) throw Error("GPU unavailable");
        this.gl = gl;
        const debug = gl.getExtension("WEBGL_debug_renderer_info");
        this.name = debug
            ? String(gl.getParameter(debug.UNMASKED_RENDERER_WEBGL))
            : String(gl.getParameter(gl.RENDERER));
        if (/swiftshader|llvmpipe|software rasterizer/i.test(this.name))
            throw Error("GPU unavailable");
        const compile = (type: number, source: string) => {
            const shader = gl.createShader(type)!;
            gl.shaderSource(shader, source);
            gl.compileShader(shader);
            if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
                const error = gl.getShaderInfoLog(shader);
                gl.deleteShader(shader);
                throw Error(error ?? "Shader compilation failed");
            }
            return shader;
        };
        const vs = compile(gl.VERTEX_SHADER, vertexShader),
            fs = compile(gl.FRAGMENT_SHADER, fragmentShader);
        this.program = gl.createProgram()!;
        gl.attachShader(this.program, vs);
        gl.attachShader(this.program, fs);
        gl.linkProgram(this.program);
        gl.deleteShader(vs);
        gl.deleteShader(fs);
        if (!gl.getProgramParameter(this.program, gl.LINK_STATUS))
            throw Error(gl.getProgramInfoLog(this.program) ?? "GPU program failed");
        for (const key of [
            "u_viewport",
            "u_origin",
            "u_scale",
            "u_texture",
            "u_mode",
            "u_premultiplied",
            "u_blur_step",
            "u_dim",
            "u_pick",
        ])
            this.uniforms[key] = gl.getUniformLocation(this.program, key);
        this.white = gl.createTexture()!;
        gl.bindTexture(gl.TEXTURE_2D, this.white);
        gl.texImage2D(
            gl.TEXTURE_2D,
            0,
            gl.RGBA,
            1,
            1,
            0,
            gl.RGBA,
            gl.UNSIGNED_BYTE,
            new Uint8Array([255, 255, 255, 255]),
        );
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
        this.overlay = this.makeDraw(
            "",
            0,
            [-1e6, -1e6, 1e6, 1e6],
            new Float32Array(),
            new Uint32Array(),
        );
        this.blurQuad = this.makeDraw("", 2, [-1e6, -1e6, 1e6, 1e6], new Float32Array(), new Uint32Array([0, 1, 2, 0, 2, 3]));
        gl.enable(gl.BLEND);
        gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
        gl.disable(gl.DEPTH_TEST);
        gl.disable(gl.CULL_FACE);
    }

    async setScene(scene: GpuScene) {
        const token = ++this.token;
        for (const key of scene.used_textures) {
            if (this.disposed || token !== this.token) return false;
            await this.loadTexture(key);
        }
        if (token !== this.token) return false;
        let uploaded = 0;
        const draws = scene.meshes.map((m) => {
            const previous = this.meshBuffers.get(m.key);
            if (previous) {
                previous.touched = ++this.frame;
                return previous.draw;
            }
            const source = meshResources.get(m.key);
            if (!source) throw Error(`Missing GPU mesh: ${m.key}`);
            source.touched = ++tick;
            const draw = this.makeDraw(m.texture, m.mode, m.clip, source.vertices, source.indices);
            this.meshBuffers.set(m.key, {
                draw,
                bytes: source.vertices.byteLength + source.indices.byteLength,
                touched: ++this.frame
            });
            uploaded++;
            return draw;
        });
        this.draws = draws;
        const meshKeys = new Set(scene.meshes.map((m) => m.key));
        let meshBytes = [...this.meshBuffers.values()].reduce((n, m) => n + m.bytes, 0);
        for (const [key, m] of [...this.meshBuffers].sort((a, b) => a[1].touched - b[1].touched)) {
            if (meshBytes <= meshBudget) break;
            if (meshKeys.has(key)) continue;
            this.freeDraw(m.draw);
            this.meshBuffers.delete(key);
            meshBytes -= m.bytes;
        }
        this.canvas.dataset.uploadedMeshes = String(uploaded);
        this.canvas.dataset.reusedMeshes = String(scene.meshes.length - uploaded);
        this.scene = scene;
        this.canvas.dataset.decodedImages = String(scene.stats.decoded_images);
        this.canvas.dataset.sceneBuildMs = scene.stats.build_ms.toFixed(3);
        const keep = new Set(scene.used_textures),
            unique = new Set(this.textures.values());
        let bytes = [...unique].reduce((v, t) => v + t.bytes, 0);
        for (const resident of [...unique].sort((a, b) => a.touched - b.touched)) {
            if (bytes <= budget) break;
            const keys = [...this.textures]
                .filter(([, value]) => value === resident)
                .map(([key]) => key);
            if (keys.some((key) => keep.has(key))) continue;
            this.gl.deleteTexture(resident.texture);
            keys.forEach((key) => this.textures.delete(key));
            bytes -= resident.bytes;
        }
        return true;
    }

    draw(view: GpuView, overlay?: Overlay) {
        if (this.disposed || this.gl.isContextLost()) return;
        const start = performance.now(),
            gl = this.gl,
            dpr = Math.min(devicePixelRatio || 1, 2);
        const width = Math.max(1, Math.round(view.width * dpr)),
            height = Math.max(1, Math.round(view.height * dpr));
        if (this.canvas.width !== width || this.canvas.height !== height) {
            this.canvas.width = width;
            this.canvas.height = height;
        }
        gl.viewport(0, 0, width, height);
        gl.disable(gl.SCISSOR_TEST);
        gl.clearColor(0, 0, 0, 0);
        gl.clear(gl.COLOR_BUFFER_BIT);
        gl.useProgram(this.program);
        gl.uniform4f(this.uniforms.u_pick, 0, 0, 0, 0);
        gl.uniform2f(this.uniforms.u_viewport, view.width, view.height);
        gl.uniform2f(this.uniforms.u_origin, view.origin.x, view.origin.y);
        gl.uniform1f(this.uniforms.u_scale, view.scale);
        gl.uniform1i(this.uniforms.u_texture, 0);
        gl.activeTexture(gl.TEXTURE0);
        gl.enable(gl.SCISSOR_TEST);
        let calls = 0;
        const render = (d: Draw, resident: Resident) => {
            const [x0, y0, x1, y1] = d.clip;
            const left = Math.max(
                    0,
                    Math.floor((x0 * view.scale + view.origin.x) * dpr),
                ),
                right = Math.min(
                    width,
                    Math.ceil((x1 * view.scale + view.origin.x) * dpr),
                ),
                top = Math.max(0, Math.floor((y0 * view.scale + view.origin.y) * dpr)),
                bottom = Math.min(
                    height,
                    Math.ceil((y1 * view.scale + view.origin.y) * dpr),
                );
            if (right <= left || bottom <= top) return;
            gl.scissor(left, height - bottom, right - left, bottom - top);
            gl.bindTexture(gl.TEXTURE_2D, resident.texture);
            gl.uniform1i(this.uniforms.u_mode, d.mode);
            gl.uniform1i(
                this.uniforms.u_premultiplied,
                resident.premultiplied ? 1 : 0,
            );
            gl.bindVertexArray(d.vao);
            gl.drawElements(gl.TRIANGLES, d.count, gl.UNSIGNED_INT, 0);
            calls++;
        };
        const underlay = this.scene?.underlay_meshes ?? 0;
        if (underlay) {
            this.prepareTargets(width, height);
            gl.bindFramebuffer(gl.FRAMEBUFFER, this.targets[0].framebuffer);
            gl.disable(gl.SCISSOR_TEST);
            gl.clear(gl.COLOR_BUFFER_BIT);
            gl.enable(gl.SCISSOR_TEST);
            for (const d of this.draws.slice(0, underlay)) {
                const t = this.textures.get(d.texture);
                if (t) render(d, t);
            }
            const x = -view.origin.x / view.scale, y = -view.origin.y / view.scale;
            const w = view.width / view.scale, h = view.height / view.scale;
            const vertex = (px: number, py: number, u: number, v: number) => [px, py, u, v, 1, 1, 1, 1, 0, 0, 0, 0];
            gl.bindBuffer(gl.ARRAY_BUFFER, this.blurQuad.vbo);
            gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([
                ...vertex(x, y, 0, 1), ...vertex(x + w, y, 1, 1),
                ...vertex(x + w, y + h, 1, 0), ...vertex(x, y + h, 0, 0),
            ]), gl.DYNAMIC_DRAW);
            const step = Math.max(0.5, view.scale * dpr * 0.03);
            gl.bindFramebuffer(gl.FRAMEBUFFER, this.targets[1].framebuffer);
            gl.disable(gl.SCISSOR_TEST);
            gl.clear(gl.COLOR_BUFFER_BIT);
            gl.enable(gl.SCISSOR_TEST);
            gl.uniform2f(this.uniforms.u_blur_step, step / width, 0);
            gl.uniform1f(this.uniforms.u_dim, 1);
            render(this.blurQuad, {texture: this.targets[0].texture, bytes: 0, touched: 0, premultiplied: true});
            gl.bindFramebuffer(gl.FRAMEBUFFER, null);
            gl.uniform2f(this.uniforms.u_blur_step, 0, step / height);
            gl.uniform1f(this.uniforms.u_dim, 0.55);
            // Keep the softened background inside the map bounds.
            this.blurQuad.clip = [0, 0, this.scene!.width, this.scene!.height];
            render(this.blurQuad, {texture: this.targets[1].texture, bytes: 0, touched: 0, premultiplied: true});
            this.blurQuad.clip = [-1e6, -1e6, 1e6, 1e6];
        }
        for (const d of this.draws.slice(underlay)) {
            const t = this.textures.get(d.texture);
            if (t) render(d, t);
        }
        if (overlay) {
            const data = overlayGeometry(overlay, view.scale);
            gl.bindBuffer(gl.ARRAY_BUFFER, this.overlay.vbo);
            gl.bufferData(
                gl.ARRAY_BUFFER,
                new Float32Array(data.vertices),
                gl.DYNAMIC_DRAW,
            );
            gl.bindVertexArray(this.overlay.vao);
            gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, this.overlay.ebo);
            gl.bufferData(
                gl.ELEMENT_ARRAY_BUFFER,
                new Uint32Array(data.indices),
                gl.DYNAMIC_DRAW,
            );
            this.overlay.count = data.indices.length;
            if (this.overlay.count)
                render(this.overlay, {
                    texture: this.white,
                    bytes: 4,
                    touched: 0,
                    premultiplied: false,
                });
            this.canvas.dataset.overlayHandles = String(overlay.items.length * 4);
        }
        gl.bindVertexArray(null);
        gl.disable(gl.SCISSOR_TEST);
        this.canvas.dataset.renderer = "webgl2";
        this.canvas.dataset.gpuRenderer = this.name;
        this.canvas.dataset.drawCalls = String(calls);
        this.canvas.dataset.frameMs = (performance.now() - start).toFixed(3);
        this.canvas.dataset.readyRevision = String(this.scene?.revision ?? -1);
    }

    /** Sample the actual textured geometry; transparent fragments let lower items through. */
    pick(point: Point, scale: number, allowed: ReadonlySet<string>): string | undefined {
        const gl = this.gl, scene = this.scene;
        if (!scene || this.disposed || gl.isContextLost() || !allowed.size) return;
        const pixelScale = scale * Math.min(devicePixelRatio || 1, 2);
        if (!this.pickTarget) {
            const texture = gl.createTexture()!, framebuffer = gl.createFramebuffer()!;
            gl.bindTexture(gl.TEXTURE_2D, texture);
            gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
            gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
            gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
            gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer);
            gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
            this.pickTarget = {texture, framebuffer};
        }
        const ids = [...allowed], colors = new Map(ids.map((id, n) => [id, n + 1]));
        gl.bindFramebuffer(gl.FRAMEBUFFER, this.pickTarget.framebuffer);
        gl.viewport(0, 0, 1, 1);
        gl.disable(gl.SCISSOR_TEST);
        gl.disable(gl.BLEND);
        gl.disable(gl.DITHER);
        gl.clearColor(0, 0, 0, 0);
        gl.clear(gl.COLOR_BUFFER_BIT);
        gl.useProgram(this.program);
        gl.uniform2f(this.uniforms.u_viewport, 1, 1);
        gl.uniform2f(this.uniforms.u_origin, 0.5 - point.x * pixelScale, 0.5 - point.y * pixelScale);
        gl.uniform1f(this.uniforms.u_scale, pixelScale);
        gl.uniform1i(this.uniforms.u_texture, 0);
        gl.activeTexture(gl.TEXTURE0);
        try {
            for (let n = scene.underlay_meshes ?? 0; n < scene.meshes.length; n++) {
                const mesh = scene.meshes[n], code = mesh.item ? colors.get(mesh.item) : undefined;
                if (code === undefined) continue;
                const [x0, y0, x1, y1] = mesh.clip;
                if (point.x < x0 || point.y < y0 || point.x >= x1 || point.y >= y1) continue;
                const draw = this.draws[n], texture = this.textures.get(draw.texture);
                if (!texture) continue;
                const [left, top, right, bottom] = draw.bounds;
                if (point.x < left || point.y < top || point.x > right || point.y > bottom) continue;
                gl.uniform4f(this.uniforms.u_pick, (code & 255) / 255, ((code >> 8) & 255) / 255, ((code >> 16) & 255) / 255, 1);
                gl.uniform1i(this.uniforms.u_mode, draw.mode);
                gl.bindTexture(gl.TEXTURE_2D, texture.texture);
                gl.bindVertexArray(draw.vao);
                gl.drawElements(gl.TRIANGLES, draw.count, gl.UNSIGNED_INT, 0);
            }
            const pixel = new Uint8Array(4);
            gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
            return ids[(pixel[0] | pixel[1] << 8 | pixel[2] << 16) - 1];
        } finally {
            gl.uniform4f(this.uniforms.u_pick, 0, 0, 0, 0);
            gl.bindFramebuffer(gl.FRAMEBUFFER, null);
            gl.bindVertexArray(null);
            gl.enable(gl.BLEND);
            gl.enable(gl.DITHER);
        }
    }

    dispose() {
        this.disposed = true;
        this.token++;
        if (this.gl.isContextLost()) return;
        for (const m of this.meshBuffers.values()) this.freeDraw(m.draw);
        this.freeDraw(this.overlay);
        this.freeDraw(this.blurQuad);
        if (this.pickTarget) {
            this.gl.deleteTexture(this.pickTarget.texture);
            this.gl.deleteFramebuffer(this.pickTarget.framebuffer);
        }
        for (const t of this.targets) {
            this.gl.deleteTexture(t.texture);
            this.gl.deleteFramebuffer(t.framebuffer);
        }
        for (const t of new Set([...this.textures.values()].map((r) => r.texture)))
            this.gl.deleteTexture(t);
        this.gl.deleteTexture(this.white);
        this.gl.deleteProgram(this.program);
    }

    private prepareTargets(width: number, height: number) {
        const size = `${width}:${height}`;
        if (this.targetSize === size) return;
        const gl = this.gl;
        for (const t of this.targets) {
            gl.deleteTexture(t.texture);
            gl.deleteFramebuffer(t.framebuffer);
        }
        this.targets = [];
        for (let i = 0; i < 2; i++) {
            const texture = gl.createTexture()!, framebuffer = gl.createFramebuffer()!;
            gl.bindTexture(gl.TEXTURE_2D, texture);
            gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
            gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
            gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
            gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
            gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
            gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer);
            gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
            this.targets.push({texture, framebuffer});
            if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) throw Error("GPU framebuffer unavailable");
        }
        gl.bindFramebuffer(gl.FRAMEBUFFER, null);
        this.targetSize = size;
    }

    private makeDraw(
        texture: string,
        mode: number,
        clip: number[],
        vertices: Float32Array,
        indices: Uint32Array,
    ): Draw {
        const gl = this.gl,
            vao = gl.createVertexArray()!,
            vbo = gl.createBuffer()!,
            ebo = gl.createBuffer()!;
        gl.bindVertexArray(vao);
        gl.bindBuffer(gl.ARRAY_BUFFER, vbo);
        gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.STATIC_DRAW);
        gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, ebo);
        gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, indices, gl.STATIC_DRAW);
        for (const [index, size, offset] of [
            [0, 2, 0],
            [1, 2, 8],
            [2, 4, 16],
            [3, 4, 32],
        ]) {
            gl.enableVertexAttribArray(index);
            gl.vertexAttribPointer(index, size, gl.FLOAT, false, 48, offset);
        }
        gl.bindVertexArray(null);
        const bounds: Draw["bounds"] = [Infinity, Infinity, -Infinity, -Infinity];
        for (let n = 0; n < vertices.length; n += 12) {
            bounds[0] = Math.min(bounds[0], vertices[n]);
            bounds[1] = Math.min(bounds[1], vertices[n + 1]);
            bounds[2] = Math.max(bounds[2], vertices[n]);
            bounds[3] = Math.max(bounds[3], vertices[n + 1]);
        }
        return {texture, mode, clip, vao, vbo, ebo, count: indices.length, bounds};
    }

    private freeDraw(d: Draw) {
        const gl = this.gl;
        gl.deleteVertexArray(d.vao);
        gl.deleteBuffer(d.vbo);
        gl.deleteBuffer(d.ebo);
    }

    private async loadTexture(key: string): Promise<Resident> {
        if (this.disposed) throw Error("GPU context lost");
        const previous = this.textures.get(key);
        if (previous) {
            previous.touched = ++this.frame;
            return previous;
        }
        const resource = resources.get(key);
        if (!resource) throw Error(`Missing GPU texture: ${key}`);
        resource.touched = ++tick;
        if (resource.alias) {
            const base = await this.loadTexture(resource.alias);
            if (this.disposed) throw Error("GPU context lost");
            this.textures.set(key, base);
            return base;
        }
        const bitmap = resource.image ? await resource.image : null;
        if (this.disposed) throw Error("GPU context lost");
        const loaded = this.textures.get(key);
        if (loaded) {
            loaded.touched = ++this.frame;
            return loaded;
        }
        const gl = this.gl;
        const texture = gl.createTexture()!;
        gl.bindTexture(gl.TEXTURE_2D, texture);
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
        gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
        gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
        if (bitmap)
            gl.texImage2D(
                gl.TEXTURE_2D,
                0,
                gl.RGBA,
                gl.RGBA,
                gl.UNSIGNED_BYTE,
                bitmap,
            );
        else
            gl.texImage2D(
                gl.TEXTURE_2D,
                0,
                gl.RGBA,
                resource.width,
                resource.height,
                0,
                gl.RGBA,
                gl.UNSIGNED_BYTE,
                resource.raw,
            );
        const wrap = resource.repeat ? gl.REPEAT : gl.CLAMP_TO_EDGE;
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, wrap);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, wrap);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
        if (bitmap) {
            gl.generateMipmap(gl.TEXTURE_2D);
            gl.texParameteri(
                gl.TEXTURE_2D,
                gl.TEXTURE_MIN_FILTER,
                gl.LINEAR_MIPMAP_LINEAR,
            );
        } else gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
        const error = gl.getError();
        if (error !== gl.NO_ERROR) {
            gl.deleteTexture(texture);
            throw Error(`GPU texture upload failed (${error})`);
        }
        const resident = {
            texture,
            bytes: resource.width * resource.height * 4 * (bitmap ? 4 / 3 : 1),
            touched: ++this.frame,
            premultiplied: resource.premultiplied,
        };
        this.textures.set(key, resident);
        return resident;
    }
}

export function overlayGeometry(overlay: Overlay, scale: number) {
    const vertices: number[] = [],
        indices: number[] = [];
    const accent = overlay.accent.map((c) => c / 255);
    const quad = (p: Point[], alpha = 1) => {
        const first = vertices.length / 12;
        for (const v of p)
            vertices.push(
                v.x,
                v.y,
                0,
                0,
                accent[0] * alpha,
                accent[1] * alpha,
                accent[2] * alpha,
                alpha,
                0,
                0,
                0,
                0,
            );
        indices.push(first, first + 1, first + 2, first, first + 2, first + 3);
    };
    const rect = (x: number, y: number, w: number, h: number, alpha = 1) =>
        quad(
            [
                {x, y},
                {x: x + w, y},
                {x: x + w, y: y + h},
                {x, y: y + h},
            ],
            alpha,
        );
    const line = (a: Point, b: Point, width = 1.5 / scale) => {
        const length = Math.hypot(b.x - a.x, b.y - a.y);
        if (length === 0) return;
        const dx = (((b.y - a.y) / length) * width) / 2,
            dy = ((-(b.x - a.x) / length) * width) / 2;
        quad([
            {x: a.x + dx, y: a.y + dy},
            {x: b.x + dx, y: b.y + dy},
            {x: b.x - dx, y: b.y - dy},
            {x: a.x - dx, y: a.y - dy},
        ]);
    };
    const outline = (x: number, y: number, w: number, h: number) => {
        const p = [
            {x, y},
            {x: x + w, y},
            {x: x + w, y: y + h},
            {x, y: y + h},
        ];
        p.forEach((a, i) => line(a, p[(i + 1) % 4]));
    };
    const circle = (p: Point, r: number) => {
        const n = Math.max(20, Math.min(96, Math.ceil(r * scale)));
        for (let i = 0; i < n; i++) {
            const a = (i * 2 * Math.PI) / n,
                b = ((i + 1) * 2 * Math.PI) / n;
            line(
                {x: p.x + Math.cos(a) * r, y: p.y + Math.sin(a) * r},
                {x: p.x + Math.cos(b) * r, y: p.y + Math.sin(b) * r},
                1 / scale,
            );
        }
    };
    for (const item of overlay.items) {
        if (overlay.geometryMode === "points" || overlay.geometryMode === "hole") {
            if (item.kind.type === "path") {
                const vs = itemPathRibbon(item);
                for (let n = 2; n < vs.length; n += 2) {
                    line(vs[n - 2].position, vs[n].position);
                    line(vs[n - 1].position, vs[n + 1].position);
                }
            }
            for (const ring of geometryRings(item)) {
                ring.slice(1).forEach((p, n) => line(ring[n], p));
                if (closedRing(item, ring)) line(ring.at(-1)!, ring[0]);
            }
            if (overlay.geometryMode === "points") {
                midpointHandles(item).forEach(v => rect(v.position.x - 2 / scale, v.position.y - 2 / scale, 4 / scale, 4 / scale, 0.65));
                vertexHandles(item).forEach(v => {
                    rect(v.position.x - 4 / scale, v.position.y - 4 / scale, 8 / scale, 8 / scale);
                    if (v.ring === overlay.vertex?.ring && v.point === overlay.vertex.point) circle(v.position, 7 / scale);
                });
                if (overlay.geometryHover) {
                    const {position: p, type} = overlay.geometryHover;
                    circle(p, 7 / scale);
                    if (type === "edge") {
                        line({x: p.x - 4 / scale, y: p.y}, {x: p.x + 4 / scale, y: p.y});
                        line({x: p.x, y: p.y - 4 / scale}, {x: p.x, y: p.y + 4 / scale});
                    }
                }
            }
            continue;
        }
        const b = bounds(item, overlay.assets),
            dx =
                overlay.drag?.mode === "move"
                    ? overlay.drag.last.x - overlay.drag.start.x
                    : 0,
            dy =
                overlay.drag?.mode === "move"
                    ? overlay.drag.last.y - overlay.drag.start.y
                    : 0;
        const width = Math.max(0.3, b.right - b.x + 0.14),
            height = Math.max(0.3, b.bottom - b.y + 0.14);
        outline((b.x + b.right - width) / 2 + dx, (b.y + b.bottom - height) / 2 + dy, width, height);
        for (const [x, y] of [
            [b.x, b.y],
            [b.right, b.y],
            [b.right, b.bottom],
            [b.x, b.bottom],
        ])
            rect(x + dx - 3 / scale, y + dy - 3 / scale, 6 / scale, 6 / scale);
    }
    const blasts = overlay.caveBlasts ?? [];
    const xs = [...new Set(blasts.flat().map(p => p.x))].sort((a, b) => a - b);
    const blastEdges = blasts.flatMap(r => r.map((p, n) => [p, r[(n + 1) % r.length]]));
    for (let n = 1; n < xs.length; n++) {
        const left = xs[n - 1], right = xs[n], middle = (left + right) / 2;
        const y = (e: Point[], x: number) => e[0].y + (x - e[0].x) * (e[1].y - e[0].y) / (e[1].x - e[0].x);
        const edges = blastEdges.filter(([a, b]) => Math.min(a.x, b.x) < middle && Math.max(a.x, b.x) > middle).sort((a, b) => y(a, middle) - y(b, middle));
        for (let k = 0; k + 1 < edges.length; k += 2) quad([{x: left, y: y(edges[k], left)}, {
            x: right,
            y: y(edges[k], right)
        },
            {x: right, y: y(edges[k + 1], right)}, {x: left, y: y(edges[k + 1], left)}], 0.25);
    }
    for (const ring of blasts) ring.forEach((p, n) => line(p, ring[(n + 1) % ring.length], 2 / scale));
    overlay.gesture
        .slice(1)
        .forEach((p, i) => line(overlay.gesture[i], p, 2 / scale));
    overlay.gesture.forEach((p) => circle(p, 3 / scale));
    if (overlay.path) {
        const {points, width, style} = overlay.path, vs = pathRibbon(points, width, 1, style);
        for (let n = 2; n < vs.length; n += 2) {
            line(vs[n - 2].position, vs[n].position);
            line(vs[n - 1].position, vs[n + 1].position);
        }
        if (vs.length) {
            line(vs[0].position, vs[1].position);
            line(vs.at(-2)!.position, vs.at(-1)!.position);
        }
    }
    if (overlay.roof) {
        const roof = roofGeometry(overlay.roof);
        roof.outline.forEach((a, n) => line(a, roof.outline[(n + 1) % roof.outline.length]));
        roof.ridge.slice(1).forEach((b, n) => line(roof.ridge[n], b, 2 / scale));
        roof.hips.forEach(([a, b]) => line(a, b));
    }
    if (overlay.drawingContour?.length) {
        const ring = overlay.drawingContour;
        ring.forEach((a, n) => line(a, ring[(n + 1) % ring.length]));
    }
    if (overlay.drag && overlay.drag.mode === "box") {
        const {start: a, last: b} = overlay.drag;
        const x = Math.min(a.x, b.x),
            y = Math.min(a.y, b.y),
            w = Math.abs(a.x - b.x),
            h = Math.abs(a.y - b.y);
        if (overlay.drag.mode === "box") rect(x, y, w, h, 0.1);
        outline(x, y, w, h);
    }
    const brush = overlay.drag?.mode === "terrain" ? overlay.drag.last : overlay.brushCursor;
    if (brush) {
        if (overlay.brushShape === "square") outline(brush.x - overlay.radius,
            brush.y - overlay.radius, overlay.radius * 2, overlay.radius * 2);
        else circle(brush, overlay.radius);
    }
    return {vertices, indices};
}

export function previewGpuScene(
    project: Project, level: number, revision: number, lights = true,
): GpuScene {
    const scene = previewFloorScene(project, level, revision, lights);
    const lower = Array.from({length: level > mainLevelIndex(project) ? level : 0}, (_, n) =>
        previewFloorScene(project, n, revision, lights));
    const meshes = lower.flatMap(s => s.meshes);
    scene.underlay_meshes = meshes.length;
    scene.meshes = [...meshes, ...scene.meshes];
    scene.textures = [...lower.flatMap(s => s.textures), ...scene.textures];
    scene.used_textures = [...new Set([...lower.flatMap(s => s.used_textures), ...scene.used_textures])];
    return scene;
}

function previewFloorScene(
    project: Project,
    level: number,
    revision: number,
    lights = true,
): GpuScene {
    const vertices: number[] = [],
        indices: number[] = [];
    const batches: { item: string | null; vertex: number; index: number; endVertex: number; endIndex: number }[] = [];
    let ownerOpacity = 1;
    const polygon = (ps: Point[], color: number[]) => {
        const first = vertices.length / 12;
        const ambient = lights ? project.ambient_light ?? [255, 255, 255, 255] : [255, 255, 255, 255];
        ps.forEach((p) =>
            vertices.push(p.x, p.y, 0, 0, ...color.map((c, i) => c / 255 * (i < 3 ? ambient[i] / 255 : 1) * ownerOpacity), 0, 0, 0, 0),
        );
        const order = ps.map((_, i) => i);
        const area = ps.reduce((n, a, i) => {
                const b = ps[(i + 1) % ps.length];
                return n + a.x * b.y - b.x * a.y;
            }, 0),
            sign = area > 0 ? 1 : -1;
        const cross = (a: Point, b: Point, c: Point) =>
            (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        let attempts = ps.length * ps.length;
        while (order.length > 2 && attempts--) {
            let found = false;
            for (let i = 0; i < order.length; i++) {
                const [a, b, c] = [
                    order[(i + order.length - 1) % order.length],
                    order[i],
                    order[(i + 1) % order.length],
                ];
                if (cross(ps[a], ps[b], ps[c]) * sign <= 0) continue;
                if (
                    order.some(
                        (n) =>
                            ![a, b, c].includes(n) &&
                            cross(ps[a], ps[b], ps[n]) * sign >= 0 &&
                            cross(ps[b], ps[c], ps[n]) * sign >= 0 &&
                            cross(ps[c], ps[a], ps[n]) * sign >= 0,
                    )
                )
                    continue;
                indices.push(first + a, first + b, first + c);
                order.splice(i, 1);
                found = true;
                break;
            }
            if (!found) break;
        }
    };
    polygon(
        [
            {x: 0, y: 0},
            {x: project.width, y: 0},
            {x: project.width, y: project.height},
            {x: 0, y: project.height},
        ],
        level === mainLevelIndex(project) ? project.background : level < mainLevelIndex(project) ? [0, 0, 0, 255] : [0, 0, 0, 0],
    );
    batches.push({item: null, vertex: 0, index: 0, endVertex: vertices.length, endIndex: indices.length});
    // Even/odd trapezoid decomposition keeps the browser preview faithful to
    // concave contours and holes without painting holes over underlying items.
    const region = (rings: Point[][], color: number[]) => {
        const xs = rings.flat().map(p => p.x).sort((a, b) => a - b).filter((x, n, a) => !n || x - a[n - 1] > 1e-7);
        const edges = rings.flatMap(r => r.map((a, n) => [a, r[(n + 1) % r.length]]));
        for (let n = 1; n < xs.length; n++) {
            const left = xs[n - 1], right = xs[n], middle = (left + right) / 2;
            const y = (edge: Point[], x: number) => edge[0].y + (x - edge[0].x) * (edge[1].y - edge[0].y) / (edge[1].x - edge[0].x);
            const cuts = edges.filter(([a, b]) => Math.min(a.x, b.x) < middle && Math.max(a.x, b.x) > middle)
                .sort((a, b) => y(a, middle) - y(b, middle));
            for (let j = 0; j + 1 < cuts.length; j += 2) {
                const ps = [{x: left, y: y(cuts[j], left)}, {x: right, y: y(cuts[j], right)},
                    {x: right, y: y(cuts[j + 1], right)}, {x: left, y: y(cuts[j + 1], left)}];
                const distinct = ps.filter((p, k) => Math.hypot(p.x - ps[(k + 3) % 4].x, p.y - ps[(k + 3) % 4].y) > 1e-7);
                if (distinct.length >= 3) polygon(distinct, color);
            }
        }
    };
    const wall = (a: Point, b: Point, width: number) => {
        const length = Math.hypot(b.x - a.x, b.y - a.y);
        if (length < 1e-7) return;
        const dx = (b.y - a.y) * width / length / 2, dy = -(b.x - a.x) * width / length / 2;
        polygon([{x: a.x + dx, y: a.y + dy}, {x: b.x + dx, y: b.y + dy}, {x: b.x - dx, y: b.y - dy}, {
            x: a.x - dx,
            y: a.y - dy
        }], [70, 75, 65, 255]);
    };
    const items = paintOrder(project.levels[level]).flatMap(prefabParts);
    const rooms = items.flatMap(i => i.kind.type === "room" ? [i.kind.rings] : []);
    const roomMask = rooms.length ? terrainContours([], 1, {regions: rooms, smoothing: 0}) : [];
    const caveMask = items.filter(i => i.cave).map(caveFootprint);
    const caveUnion = caveMask.length ? terrainContours([], 1, {regions: caveMask, smoothing: 0}) : [];
    for (const i of items) {
        const owner = i.id.split(":part:")[0];
        ownerOpacity = project.levels[level].layers.find(l => l.items.some(item => item.id === owner))?.opacity ?? 1;
        const batch = {item: owner, vertex: vertices.length, index: indices.length, endVertex: 0, endIndex: 0};
        if (i.cave && i.kind.type === "terrain_region") {
            region(terrainContours([], 1, {existing: caveFootprint(i), subtract: [roomMask], smoothing: 0}),
                i.cave.colorable === false ? [127, 126, 113, 255] : i.cave.ground_color);
            const masks = terrainContours([], 1, {regions: [i.cave.blasts, roomMask], smoothing: 0});
            for (const ring of i.kind.rings) ring.forEach((p, n) => {
                for (const [a, b] of outsideCaveMasks(p, ring[(n + 1) % ring.length], masks, true)) wall(a, b, 0.5);
            });
        }
        if (i.kind.type === "room") {
            region(i.kind.rings, [185, 179, 155, 255]);
            const width = i.kind.width;
            i.kind.rings.forEach((ring, n) => {
                if (!(i.wall_exclusions ?? []).includes(n) && i.kind.type === "room" && i.kind.wall)
                    ring.forEach((p, j) => outsideCaveMasks(p, ring[(j + 1) % ring.length], caveUnion).forEach(([a, b]) => wall(a, b, width)));
            });
        }
        if (i.kind.type === "floor") region(geometryRings(i), i.tint);
        if (i.kind.type === "roof") {
            const k = i.kind, roof = roofGeometry(k);
            roof.facets.forEach(f => {
                const sun = k.sun_direction * Math.PI / 180;
                const brightness = k.shade ? 1 - k.contrast * (1 - f.down.x * Math.cos(sun) - f.down.y * Math.sin(sun)) / 2 : 1;
                polygon(f.points, i.tint.map((v, n) => n < 3 ? v * brightness : v));
            });
            roof.ridge.slice(1).forEach((p, n) => wall(roof.ridge[n], p, 0.08));
            roof.hips.forEach(([a, b]) => wall(a, b, 0.04));
        }
        if (i.kind.type === "path") {
            const vs = itemPathRibbon(i);
            for (let n = 2; n < vs.length; n += 2) polygon([vs[n - 2].position, vs[n].position, vs[n + 1].position, vs[n - 1].position], i.tint.map((v, k) => k === 3 ? v * (vs[n - 2].alpha + vs[n].alpha) / 2 : v));
        }
        if (i.kind.type === "wall") {
            const {points, width} = i.kind;
            points.slice(1).forEach((b, n) => wall(points[n], b, width));
        }
        batch.endVertex = vertices.length;
        batch.endIndex = indices.length;
        if (batch.endIndex > batch.index) batches.push(batch);
    }
    const encode = (bytes: Uint8Array) => {
        let binary = "";
        for (const b of bytes) binary += String.fromCharCode(b);
        return btoa(binary);
    };
    const key = "preview:white";
    const labels = project.levels[level].layers.filter(l => l.visible).flatMap(l => l.items.map(i =>
        previewText(i, project.pixels_per_grid, revision, lights ? project.ambient_light ?? [255, 255, 255, 255] : [255, 255, 255, 255], l.opacity))).filter(v => v !== null);
    return {
        generation: "preview",
        revision,
        width: project.width,
        height: project.height,
        meshes: [
            ...batches.map((batch, n) => ({
                key: `preview:${project.id}:${level}:${revision}:${n}`,
                item: batch.item,
                texture: key,
                mode: 0,
                clip: [0, 0, project.width, project.height],
                vertices: encode(new Uint8Array(new Float32Array(vertices.slice(batch.vertex, batch.endVertex)).buffer)),
                indices: encode(new Uint8Array(new Uint32Array(indices.slice(batch.index, batch.endIndex).map(i => i - batch.vertex / 12)).buffer)),
                vertex_count: (batch.endVertex - batch.vertex) / 12,
                index_count: batch.endIndex - batch.index,
            })),
            ...labels.map(v => v.mesh),
        ],
        textures: [...(resources.has(key)
            ? []
            : [
                {
                    key,
                    width: 1,
                    height: 1,
                    data: "/////w==",
                    format: "rgba",
                    repeat: false,
                    premultiplied: false,
                    alias: null,
                },
            ]), ...labels.map(v => v.texture)],
        used_textures: [key, ...labels.map(v => v.texture.key)],
        stats: {
            build_ms: 0,
            decoded_images: 0,
            cache_hits: 0,
            vertices: vertices.length / 12,
            texture_bytes: 4,
        },
    };
}
