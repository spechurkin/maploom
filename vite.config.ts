import {defineConfig} from "vite";
import {build} from "esbuild";

export default defineConfig({
    root: "frontend",
    clearScreen: false,
    plugins: [{
        name: "maploom-theme-sandbox",
        resolveId(id) {
            if (id === "virtual:maploom-theme-sandbox") return "\0" + id;
        },
        async load(id) {
            if (id !== "\0virtual:maploom-theme-sandbox") return;
            const result = await build({
                entryPoints: ["frontend/src/themes/sandbox.ts"], bundle: true, write: false,
                format: "iife", platform: "browser", target: "es2022", minify: true,
                define: {"process.env.NODE_ENV": '"production"'},
                external: ["fs", "path", "os", "crypto", "perf_hooks", "inspector", "source-map-support"],
            });
            for (const file of ["sandbox.ts", "compiler.ts", "contract.ts"]) this.addWatchFile(`frontend/src/themes/${file}`);
            return `export default ${JSON.stringify(result.outputFiles[0].text)}`;
        },
    }],
    server: {host: "127.0.0.1", port: 1420, strictPort: true},
    build: {outDir: "dist", emptyOutDir: true},
});
