import {
    type PointerEvent as ReactPointerEvent,
    type ReactNode,
    useCallback,
    useEffect,
    useMemo,
    useRef,
    useState,
} from "react";
import {
    AlertCircle,
    ArrowDown,
    ArrowUp,
    BrickWall,
    Check,
    CheckCheck,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    ChevronsDown,
    ChevronsUp,
    Circle,
    Compass,
    Copy,
    DoorOpen,
    Download,
    Droplets,
    Eraser,
    Eye,
    EyeOff,
    FilePlus2,
    FlipHorizontal,
    FlipVertical,
    FolderOpen,
    Grid2X2,
    Hand,
    History,
    House,
    ImagePlus,
    Keyboard,
    Layers,
    Lightbulb,
    LoaderCircle,
    LockKeyhole,
    LockKeyholeOpen,
    Magnet,
    Map as MapIcon,
    Maximize,
    Minus,
    Mountain,
    MousePointer2,
    Package,
    PanelRightClose,
    PanelRightOpen,
    Pentagon,
    Plus,
    Redo2,
    RotateCw,
    Route,
    Save,
    Scissors,
    Search,
    Settings2,
    Shuffle,
    SlidersHorizontal,
    Square,
    Star,
    Sun,
    Trash2,
    Trees,
    Type,
    Undo2,
    X,
} from "lucide-react";
import {listen} from "@tauri-apps/api/event";
import {useUpdates} from "./useUpdates";
import {UpdatesPanel} from "../panels/UpdatesPanel";
import {useThemes} from "../themes/useThemes";
import {ThemeSettings} from "../themes/ThemeSettings";
import * as bridge from "./bridge";
import {type Action} from "./bridge";
import {TextEditor, type TextKind, type TextMode} from "../panels/TextEditor";
import {PrefabsPanel} from "../panels/PrefabsPanel";
import {PrefabPlacement} from "../rendering/PrefabPreview";
import {canCreatePrefab} from "../map/prefabs";
import {VariationsPanel} from "../panels/VariationsPanel";
import {rememberTextBounds} from "../tools/text";
import type {CaveStyle} from "../map/model";
import {
    type Asset,
    type AssetCatalog,
    bounds,
    boxSelection,
    catalogPackEnabled,
    catalogSelectionState,
    defaultTextStyle,
    displayDirectoryPath,
    duplicatePackFiles,
    hex,
    type Item,
    type Layer,
    moveToLayer,
    newItem,
    newLayer,
    type Point,
    portalType,
    type Preferences,
    type Project,
    reorderItems,
    rgba,
    roomTint,
    setCatalogPack,
    setCatalogEnabled,
    setObjectSize,
    setPortalStyle,
    setRoomTint,
    type Snapshot,
    type TextStyle,
    type Tool,
    type WaterStyle
} from "../map/model";
import {
    addHole,
    circleContour,
    type DrawShape,
    type GeometryMode,
    geometryRings,
    type GeometryTarget,
    geometryTarget,
    insertVertex,
    moveVertex,
    oriented,
    rectangle,
    removeHole,
    removeVertex,
    resizeFromCorner,
    resizeGeometry,
    samePoint,
    setContourWalls,
    signedArea,
    supportsGeometry,
    validGeometry,
    type Vertex
} from "../tools/architecture";
import {translator} from "./i18n";
import {resetThumbnails, Thumbnail} from "../components/Thumbnail";
import {GpuCanvas} from "../rendering/GpuCanvas";
import {FloorsPanel} from "../panels/FloorsPanel";
import {mainLevelIndex} from "../map/levels";
import {ObjectPlacement, objectPlacement} from "../rendering/ObjectPlacement";
import {PortalPlacement, type PortalPreviewResult, portalPreviewScope} from "../rendering/PortalPlacement";
import {DrawingPreview, type DrawingPreviewResult} from "../rendering/DrawingPreview";
import {drawingPoints} from "../tools/drawingGeometry";
import {ColorField} from "../components/ColorField";
import {AddColorButton} from "../components/AddColorButton";
import {ProjectSetup} from "../panels/ProjectSetup";
import {MapResize} from "../panels/MapResize";
import {RangeField} from "../components/RangeField";
import {DualRangeField} from "../components/DualRangeField";
import {
    chooseScatterAssets,
    extendScatterStroke,
    scatterItem,
    type ScatterSettings,
    type ScatterStroke
} from "../tools/scatter";
import type {GpuRenderer, GpuScene} from "../rendering/gpu";
import brand from "../../../assets/branding/maploom-icon.png";
import logo from "../../../assets/branding/maploom-logo.svg";
import logoDark from "../../../assets/branding/maploom-logo-dark.svg";
import {shortcutKey} from "./shortcuts";
import {addTerrainRegion, appendTerrainPoint, terrainContours, type WaterShape} from "../tools/terrain";
import {paintWaterOnLevel, setWaterStyle} from "../tools/water";
import {caveOpenedAreas, caveStyles, paintCave} from "../tools/caves";
import {snapObjectsToWalls, snapWalls} from "../tools/wallSnap";
import {defaultPathStyle, type PathStyle, type PathTransition, setPathTexture, simplifyPath} from "../tools/paths";
import {
    newRoof,
    type RoofKind,
    roofQuickbox,
    roofStyle,
    roofStyleAsset,
    roofStyleName,
    type RoofType
} from "../tools/roofs";

const toolGroups = [
    {id: "architecture", label: "Architecture", icon: House},
    {id: "landscape", label: "Terrain", icon: Mountain},
    {id: "objects", label: "Objects", icon: Trees},
    {id: "lighting", label: "Lights", icon: Lightbulb},
] as const;
type ToolGroup = typeof toolGroups[number]["id"];
const tools: { id: Tool; label: string; key: string; icon: typeof Square; group?: ToolGroup }[] = [
    {id: "select", label: "Select", key: "V", icon: MousePointer2},
    {id: "pan", label: "Pan", key: "H", icon: Hand},
    {id: "erase", label: "Erase", key: "E", icon: Eraser},
    {id: "room", label: "Room", key: "R", icon: Square, group: "architecture"},
    {id: "wall", label: "Walls", key: "W", icon: BrickWall, group: "architecture"},
    {id: "floor", label: "Floor", key: "G", icon: Layers, group: "architecture"},
    {id: "portal", label: "Portal tool", key: "D", icon: DoorOpen, group: "architecture"},
    {id: "roof", label: "Roof", key: "K", icon: House, group: "architecture"},
    {id: "terrain", label: "Terrain", key: "B", icon: Mountain, group: "landscape"},
    {id: "cave", label: "Cave", key: "C", icon: Mountain, group: "landscape"},
    {id: "path", label: "Paths", key: "P", icon: Route, group: "landscape"},
    {id: "object", label: "Objects", key: "O", icon: Trees, group: "objects"},
    {id: "scatter", label: "Scatter", key: "S", icon: Shuffle, group: "objects"},
    {id: "prefab", label: "Prefabs", key: "", icon: Package, group: "objects"},
    {id: "light", label: "Light", key: "L", icon: Lightbulb, group: "lighting"},
    {id: "text", label: "Text", key: "T", icon: Type},
];
const categories: Record<string, string> = {
    objects: "Object",
    tilesets: "Floor",
    walls: "Wall",
    terrain: "Terrain",
    portals: "Portal",
    roofs: "Roof",
    paths: "Path",
};
const toolCategory: Partial<Record<Tool, string>> = {
    object: "objects",
    scatter: "objects",
    floor: "tilesets",
    wall: "walls",
    terrain: "terrain",
    portal: "portals",
    room: "tilesets",
    roof: "roofs",
    path: "paths",
};
const defaultToolLayers: Partial<Record<Tool, string>> = {
    object: "Objects",
    scatter: "Objects",
    prefab: "Objects",
    light: "Lighting",
    wall: "Architecture",
    floor: "Architecture",
    room: "Architecture",
    cave: "Architecture",
    roof: "Architecture",
    terrain: "Ground",
    path: "Ground",
    portal: "Ground",
    text: "Ground",
};
const white: [number, number, number, number] = [255, 255, 255, 255];
type Modal =
    | "new"
    | "export"
    | "settings"
    | "resize-map"
    | "map-settings"
    | "commands"
    | "help"
    | "report"
    | "catalogs"
    | "fabric"
    | "prefab"
    | "asset"
    | null;
type Drag = {
    start: Point;
    last: Point;
    screen: Point;
    mode: "pan" | "room" | "move" | "box" | "terrain" | "erase" | "vertex" | "resize" | "hole" | "scatter" | "path";
    ids: string[];
    trail: Point[];
    additive: boolean;
    source?: Item;
    vertex?: Vertex;
    corner?: number;
    rawLast?: Point;
    rawStart?: Point;
    scatter?: ScatterStroke;
    items?: Item[];
    revision?: number;
};

function IconButton({
                        label,
                        children,
                        active,
                        onClick,
                        disabled = false,
                        className = "",
                    }: {
    label: string;
    children: ReactNode;
    active?: boolean;
    onClick?: () => void;
    disabled?: boolean;
    className?: string;
}) {
    return (
        <button
            className={`icon-button ${active ? "active" : ""} ${className}`}
            title={label}
            aria-label={label}
            aria-pressed={active}
            onClick={onClick}
            disabled={disabled}
        >
            {children}
        </button>
    );
}

function NumberField({
                         label,
                         value,
                         onChange,
                         min = 0,
                         max = 512,
                         step = 1,
                         disabled = false,
                     }: {
    label: string;
    value: number;
    onChange: (n: number) => void;
    min?: number;
    max?: number;
    step?: number;
    disabled?: boolean;
}) {
    return (
        <label className="field">
            <span>{label}</span>
            <input
                type="number"
                key={value}
                defaultValue={Math.round(value * 100) / 100}
                min={min}
                max={max}
                step={step}
                disabled={disabled}
                onBlur={(e) => {
                    const n = Number(e.target.value);
                    if (
                        Number.isFinite(n) &&
                        e.target.value !== "" &&
                        n >= min &&
                        n <= max &&
                        n !== value
                    )
                        onChange(n);
                    else e.target.value = String(value);
                }}
                onKeyDown={(e) => {
                    if (e.key === "Enter") e.currentTarget.blur();
                }}
            />
        </label>
    );
}

function Toggle({
                    label,
                    checked,
                    onChange,
                }: {
    label: string;
    checked: boolean;
    onChange: (v: boolean) => void;
}) {
    return (
        <label className="toggle-row">
            <span>{label}</span>
            <input
                type="checkbox"
                checked={checked}
                onChange={(e) => onChange(e.target.checked)}
            />
            <span className="switch"/>
        </label>
    );
}

function CaveFields({value, onChange, styles, material, onMaterial, t}: {
    value: CaveStyle; onChange: (v: CaveStyle) => void; styles: ReturnType<typeof caveStyles>;
    material: string; onMaterial: (s: ReturnType<typeof caveStyles>[number]) => void; t: (k: string) => string;
}) {
    const colorable = styles.find(s => s.id === material)?.colorable ?? value.colorable ?? true;
    return <div className="cave-fields">
        <ColorField disabled={!colorable} label={t("Ground color")} value={hex(value.ground_color)}
                    onChange={c => onChange({...value, ground_color: rgba(c)})}/>
        <ColorField disabled={!colorable} label={t("Wall color")} value={hex(value.wall_color)}
                    onChange={c => onChange({...value, wall_color: rgba(c)})}/>
        <span className="muted tiny">{t("Style")}</span>
        <div className="cave-styles" role="group" aria-label={t("Cave style")}>
            {styles.map(s => <button key={s.id} className={s.id === material ? "active" : ""}
                                     title={t(s.name)} aria-label={t(s.name)} aria-pressed={s.id === material}
                                     onClick={() => onMaterial(s)}>
                <Thumbnail id={s.floor.meta.id}
                           color={s.colorable ? value.ground_color : white}/><span>{t(s.name)}</span>
            </button>)}
        </div>
        {!styles.length && <p className="muted tiny">{t("No cave styles")}</p>}
    </div>;
}

function WaterFields({value, onChange, t}: {
    value: WaterStyle; onChange: (value: WaterStyle) => void; t: (key: string) => string;
}) {
    return <div className="water-fields">
        <ColorField label={t("Deep color")} value={hex(value.deep_color)}
                    onChange={c => onChange({...value, deep_color: rgba(c)})}/>
        <ColorField label={t("Shallow color")} value={hex(value.shallow_color)}
                    onChange={c => onChange({...value, shallow_color: rgba(c)})}/>
        <RangeField label={t("Blend distance")} unit={t("cells")} value={value.blend_distance} min={0} step={0.1}
                    onChange={blend_distance => onChange({...value, blend_distance})}/>
        <Toggle label={t("Disable border")} checked={value.disable_border}
                onChange={disable_border => onChange({...value, disable_border})}/>
        <RangeField label={t("Water smoothing")} unit="%" value={(value.smoothing ?? 0.2) * 100} min={0} max={100}
                    step={5}
                    onChange={smoothing => onChange({...value, smoothing: smoothing / 100})}/>
    </div>;
}

type LightSettings = Pick<Extract<Item["kind"], { type: "light" }>, "range" | "intensity" | "color" | "smoothing">;

function PathFields({t, width, style, onChange}: {
    t: (key: string) => string;
    width: number;
    style: PathStyle;
    onChange: (width: number, style: PathStyle) => void
}) {
    return <>
        <RangeField label={t("Width")} unit="" min={0.001} max={Math.max(5, width)} step={0.01} value={width}
                    onChange={w => onChange(w, style)}/>
        <RangeField label={t("Path smoothness")} unit="" min={0} max={1} step={0.05} value={style.smoothness}
                    onChange={smoothness => onChange(width, {...style, smoothness})}/>
        <p className="muted tiny">{t("Path smoothness hint")}</p>
        <Toggle label={t("Block light")} checked={style.blocks_light}
                onChange={blocks_light => onChange(width, {...style, blocks_light})}/>
        {(["transition_in", "transition_out"] as const).map(key => <label className="field"
                                                                          key={key}><span>{t(key === "transition_in" ? "Transition in" : "Transition out")}</span>
            <select value={style[key]}
                    onChange={e => onChange(width, {...style, [key]: e.target.value as PathTransition})}>
                <option value="none">{t("No transition")}</option>
                <option value="fade">{t("Fade")}</option>
                <option value="shrink">{t("Shrink")}</option>
            </select></label>)}
    </>;
}

type RoofSettings = Pick<RoofKind, "width" | "roof_type" | "shade" | "sun_direction" | "contrast">;

function RoofFields({value, onChange, t, showWidth = true}: {
    value: RoofSettings;
    onChange: (v: RoofSettings) => void;
    t: (key: string) => string;
    showWidth?: boolean
}) {
    return <>
        <label className="field"><span>{t("Roof type")}</span><select value={value.roof_type}
                                                                      onChange={e => onChange({
                                                                          ...value,
                                                                          roof_type: e.target.value as RoofType
                                                                      })}>
            {([['gable', 'Gable'], ['hip', 'Hip'], ['dormer', 'Dormer']] as const).map(([id, label]) => <option key={id}
                                                                                                                value={id}>{t(label)}</option>)}
        </select></label>
        {showWidth &&
            <RangeField label={t("Roof width")} unit={t("cells")} min={0.05} max={512} step={0.25} value={value.width}
                        onChange={width => onChange({...value, width})}/>}
        <Toggle label={t("Shade")} checked={value.shade} onChange={shade => onChange({...value, shade})}/>
        {value.shade && <>
            <RangeField label={t("Sun direction")} unit="°" min={0} max={360} step={5} value={value.sun_direction}
                        onChange={sun_direction => onChange({...value, sun_direction})}/>
            <RangeField label={t("Shade contrast")} unit="%" min={0} max={100} step={5} value={value.contrast * 100}
                        onChange={contrast => onChange({...value, contrast: contrast / 100})}/>
        </>}
    </>;
}

function LightFields({value, onChange, t}: {
    value: LightSettings; onChange: (value: LightSettings) => void; t: (key: string) => string;
}) {
    return <div className="light-fields">
        <RangeField label={t("Intensity")} unit="" value={Math.min(value.intensity, 2)} min={0} max={2} step={0.1}
                    onChange={intensity => onChange({...value, intensity})}/>
        <RangeField label={t("Size")} unit="" value={value.range} min={0.1} max={100} step={0.1}
                    onChange={range => onChange({...value, range})}/>
        <ColorField label={t("Color")} value={hex(value.color)}
                    onChange={color => onChange({...value, color: rgba(color)})}/>
        <RangeField label={t("Light smoothing")} unit="%" value={(value.smoothing ?? 1) * 100} min={0} max={100}
                    step={1}
                    onChange={smoothing => onChange({...value, smoothing: smoothing / 100})}/>
    </div>;
}

export function App() {
    const updates = useUpdates();
    const [settingsTab, setSettingsTab] = useState<"appearance" | "localization" | "updates" | "general">("appearance");
    const [data, setData] = useState<Snapshot | null>(null),
        current = useRef<Snapshot | null>(null),
        queue = useRef(Promise.resolve());
    const [catalogs, setCatalogs] = useState<AssetCatalog[]>([]);
    const duplicatePacks = duplicatePackFiles(catalogs);
    const [assets, setAssets] = useState<Asset[]>([]),
        [busy, setBusy] = useState(0),
        [error, setError] = useState(""),
        [toast, setToast] = useState("");
    const assetGeometry = useMemo(() => Object.fromEntries(assets.map(a => [a.meta.id, a.meta])), [assets]);
    const [tool, setTool] = useState<Tool>("select"),
        [level, setLevel] = useState(0),
        [layer, setLayer] = useState(1),
        [selected, setSelected] = useState<string[]>([]);
    const toolGroup = tools.find(o => o.id === tool)?.group;
    const hasToolControls = tool !== "pan";
    const lastGroupTools = useRef<Partial<Record<ToolGroup, Tool>>>({});
    useEffect(() => {
        const group = tools.find(o => o.id === tool)?.group;
        if (group) {
            lastGroupTools.current[group] = tool;
        }
    }, [tool]);
    const [grid, setGrid] = useState(true),
        [snap, setSnap] = useState(true),
        [wallSnap, setWallSnap] = useState(false),
        [lights, setLights] = useState(true),
        [right, setRight] = useState(true),
        [tab, setTab] = useState<"library" | "inspector">("library");
    const [scale, setScale] = useState(24),
        [pan, setPan] = useState<Point>({x: 0, y: 0}),
        [viewport, setViewport] = useState({w: 800, h: 700}),
        stage = useRef<HTMLDivElement>(null);
    const picker = useRef<GpuRenderer | null>(null);
    const [gpuScene, setGpuScene] = useState<GpuScene | null>(null),
        [sceneEpoch, setSceneEpoch] = useState(0),
        [rendering, setRendering] = useState(false),
        [sceneError, setSceneError] = useState("");
    const [recoveryDismissed, setRecoveryDismissed] = useState(false);
    const [modal, setModal] = useState<Modal>(null),
        [fileMenu, setFileMenu] = useState(false),
        [query, setQuery] = useState(""),
        [commandQuery, setCommandQuery] = useState(""),
        [category, setCategory] = useState("objects"),
        [page, setPage] = useState(0),
        [favoriteOnly, setFavoriteOnly] = useState(false);
    const [favorites, setFavorites] = useState<string[]>(() => {
        try {
            return JSON.parse(localStorage.getItem("maploom:favorites") ?? "[]");
        } catch {
            return [];
        }
    });
    const [materials, setMaterials] = useState<Record<string, string | null>>({}),
        [materialTab, setMaterialTab] = useState<"tilesets" | "walls">("tilesets");
    const [color, setColor] = useState("#ffffff"),
        [floorColor, setFloorColor] = useState("#ffffff"),
        [wallColor, setWallColor] = useState("#ffffff"),
        [floorScale, setFloorScale] = useState(1),
        [objectScale, setObjectScale] = useState(1),
        [objectBlocksLight, setObjectBlocksLight] = useState(false),
        [rotation, setRotation] = useState(0),
        [width, setWidth] = useState(0.18),
        [radius, setRadius] = useState(2);
    const [textMode, setTextMode] = useState<TextMode>("edit");
    const [prefabKey, setPrefabKey] = useState("");
    const [prefabName, setPrefabName] = useState("");
    const [scatterIds, setScatterIds] = useState<string[]>([]);
    const [scatterSettings, setScatterSettings] = useState<ScatterSettings>({
        rotation: [-180, 180], scale: [1, 1], spread: 0.5, randomColors: false,
        colors: [
            "#713f39", "#ad5651", "#c77e5e", "#aa9464", "#788957", "#40634a",
            "#247e78", "#2977a8", "#405483", "#715582", "#ac6482", "#d49c9c",
            "#e2bd77", "#d7ceb2", "#f1eee5", "#9ca6ad", "#626872", "#343841",
        ].map(rgba),
        color: white,
    });
    const [scatterEpoch, setScatterEpoch] = useState(0);
    const scatterAssets = useMemo(() => assets.filter(a => a.category === "objects" &&
            (!a.builtin || data?.project.use_default_assets) && scatterIds.includes(a.meta.id)).map(a => a.meta),
        [assets, data?.project.use_default_assets, scatterIds]);
    const nextScatter = useMemo(() => scatterItem(scatterAssets, {x: 0, y: 0}, {
            ...scatterSettings,
            color: rgba(color)
        }),
        [scatterAssets, scatterSettings, color, scatterEpoch]);
    const [textDraft, setTextDraft] = useState<TextKind>({
        type: "text",
        position: {x: 0, y: 0},
        text: "",
        size: 0.35,
        color: [0, 0, 0, 255]
    });
    const [textStyle, setTextStyle] = useState<TextStyle>(defaultTextStyle);
    const [drawShapes, setDrawShapes] = useState<Record<string, DrawShape>>({
        room: "rectangle",
        floor: "polygon",
        wall: "polygon"
    });
    const [pathMode, setPathMode] = useState<"points" | "freehand" | "edit">("points"),
        [pathStyle, setPathStyle] = useState<PathStyle>({...defaultPathStyle(), smoothness: 1}),
        [pathWidth, setPathWidth] = useState(0.5), [pathUnder, setPathUnder] = useState(false),
        [pathScale, setPathScale] = useState(1);
    const [roofMode, setRoofMode] = useState<"quickbox" | "manual">("quickbox"),
        [roofType, setRoofType] = useState<RoofType>("gable"),
        [roofWidth, setRoofWidth] = useState(3),
        [roofUnder, setRoofUnder] = useState(false),
        [roofShade, setRoofShade] = useState(true),
        [roofSun, setRoofSun] = useState(135),
        [roofContrast, setRoofContrast] = useState(0.3);
    const [segmentation, setSegmentation] = useState(64), [newRoomWalls, setNewRoomWalls] = useState(true);
    const [geometryMode, setGeometryMode] = useState<GeometryMode | null>(null),
        [vertex, setVertex] = useState<Vertex | null>(null), [holeShape, setHoleShape] = useState<DrawShape>("polygon"),
        [holeWalls, setHoleWalls] = useState(false);
    const [geometryDraft, setGeometryDraft] = useState<Item | null>(null);
    const pathEditing = tool === "path" && pathMode === "edit";
    const [geometryHover, setGeometryHover] = useState<GeometryTarget | null>(null);
    useEffect(() => {
        setGeometryHover(null);
    }, [geometryMode, selected, tool, data?.revision]);
    const drawShape = tool === "roof" ? roofMode === "quickbox" ? "rectangle" : "polygon" : drawShapes[tool] ?? "polygon";
    const [brushCursor, setBrushCursor] = useState<Point | null>(null);
    const [placementCursor, setPlacementCursor] = useState<Point | null>(null);
    const [roofCursor, setRoofCursor] = useState<Point | null>(null);
    const [portalScene, setPortalScene] = useState<PortalPreviewResult | null>(null);
    const [drawingScene, setDrawingScene] = useState<DrawingPreviewResult | null>(null);
    const [light, setLight] = useState<LightSettings>({
        range: 3, intensity: 0.4,
        color: [255, 212, 131, 255], smoothing: 1
    });
    const [lightTab, setLightTab] = useState<"sources" | "environment">("sources");
    const [gesture, setGesture] = useState<Point[]>([]),
        [drag, setDrag] = useState<Drag | null>(null),
        dragRef = useRef<Drag | null>(null),
        space = useRef(false);
    const [terrainMode, setTerrainMode] = useState<"terrain" | "water">("terrain"),
        [waterShape, setWaterShape] = useState<WaterShape>("circle"),
        [water, setWater] = useState<WaterStyle>({
            deep_color: [55, 159, 154, 255],
            shallow_color: [141, 203, 177, 255], blend_distance: 1.5, disable_border: false, smoothing: 0.2
        });
    const [caveMode, setCaveMode] = useState<"dig" | "blast">("dig"),
        [showBlasts, setShowBlasts] = useState(false), [caveMaterial, setCaveMaterial] = useState("");
    const [cave, setCave] = useState<CaveStyle>({
        colorable: true,
        ground_color: [127, 126, 113, 255],
        wall_color: [127, 126, 113, 255],
        blasts: []
    });
    const caveMaterials = useMemo(() => caveStyles(assets.filter(a => !a.builtin || data?.project.use_default_assets)),
        [assets, data?.project.use_default_assets]);
    const activeCaveMaterial = caveMaterials.find(s => s.id === caveMaterial) ?? caveMaterials[0];
    const [exportFormat, setExportFormat] = useState("png"),
        [exportAll, setExportAll] = useState(false),
        [exportVariations, setExportVariations] = useState(false),
        [ppg, setPpg] = useState(100),
        [exportGrid, setExportGrid] = useState(false),
        [exportLights, setExportLights] = useState(true),
        [quality, setQuality] = useState(92);
    const [fabricName, setFabricName] = useState("My Fabric"),
        [fabricAuthor, setFabricAuthor] = useState(""),
        [fabricIds, setFabricIds] = useState<string[]>([]),
        [crop, setCrop] = useState([0, 0, 64, 64]),
        [variantName, setVariantName] = useState(""),
        [flipX, setFlipX] = useState(false),
        [flipY, setFlipY] = useState(false),
        [turns, setTurns] = useState(0);
    const t = translator(data?.preferences.locale ?? "en", data?.locales);
    const themes = useThemes(data?.preferences, !!data?.project_open);
    const exportNeedsAlpha = exportAll || (!exportVariations && !!data && level !== mainLevelIndex(data.project));
    useEffect(() => {
        if (exportNeedsAlpha && exportFormat === "jpeg") setExportFormat("png");
    }, [exportNeedsAlpha, exportFormat]);
    const levelRef = useRef(level);
    levelRef.current = level;
    const accept = useCallback((s: Snapshot) => {
        if (!current.current || current.current.project.id !== s.project.id ||
            current.current.project.active_variation !== s.project.active_variation) {
            setSelected([]);
            const main = mainLevelIndex(s.project);
            setLevel(main);
            setLayer(Math.min(1, s.project.levels[main].layers.length - 1));
            setGesture([]);
        } else {
            const id = current.current.project.levels[levelRef.current]?.id;
            const index = s.project.levels.findIndex(l => l.id === id);
            const next = index < 0 ? mainLevelIndex(s.project) : index;
            setLevel(next);
            setLayer(n => Math.min(n, s.project.levels[next].layers.length - 1));
            if (index < 0) setSelected([]);
        }
        current.current = s;
        setData(s);
        if (s.catalogs) setCatalogs(s.catalogs);
        if (s.assets) {
            resetThumbnails();
            setAssets(s.assets);
        }
    }, []);
    useEffect(() => {
        let alive = true;
        bridge.snapshot().then(
            (s) => {
                if (alive) accept(s);
            },
            (e) => {
                if (alive) setError(String(e));
            },
        );
        return () => {
            alive = false;
        };
    }, [accept]);
    const run = useCallback(
        (action: Action | ((s: Snapshot) => Action), success = "") => {
            if (updates.installing) return Promise.resolve(false);
            setBusy((n) => n + 1);
            const task = queue.current.then(async () => {
                const s = current.current;
                if (!s) return;
                const a = typeof action === "function" ? action(s) : action;
                const next = await bridge.act(a, s.revision);
                accept(next);
                if (success) setToast(success);
            });
            queue.current = task
                .catch((e) => {
                    if (!String(e).includes("Cancelled")) setError(String(e));
                })
                .finally(() => setBusy((n) => n - 1));
            return task.then(
                () => true,
                () => false,
            );
        },
        [accept, updates.installing],
    );
    const edit = useCallback(
        (change: (p: Project) => void) =>
            run((s) => {
                const p = structuredClone(s.project);
                change(p);
                return {type: "commit", project: p};
            }),
        [run],
    );
    useEffect(() => {
        if (modal === "catalogs") void run({type: "load_library"});
    }, [modal, run]);
    const pref = (change: Partial<Preferences>) =>
        run((s) => ({
            type: "preferences",
            preferences: {...s.preferences, ...change},
        }));
    useEffect(() => {
        if (!toast) return;
        const timeout = setTimeout(() => setToast(""), 3500);
        return () => clearTimeout(timeout);
    }, [toast]);
    useEffect(() => {
        if (!modal) return;
        const previous = document.activeElement as HTMLElement;
        const container = document.querySelector<HTMLElement>(".modal");
        const focusable = () =>
            Array.from(
                container?.querySelectorAll<HTMLElement>(
                    'button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),[tabindex="0"]',
                ) ?? [],
            );
        const first =
            container?.querySelector<HTMLElement>("[autofocus]") ?? focusable()[0];
        first?.focus();
        const trap = (e: KeyboardEvent) => {
            if (e.key !== "Tab") return;
            const list = focusable();
            if (!list.length) return;
            const index = list.indexOf(document.activeElement as HTMLElement);
            if (e.shiftKey && index <= 0) {
                e.preventDefault();
                list.at(-1)?.focus();
            } else if (!e.shiftKey && index === list.length - 1) {
                e.preventDefault();
                list[0].focus();
            }
        };
        document.addEventListener("keydown", trap);
        return () => {
            document.removeEventListener("keydown", trap);
            previous?.focus();
        };
    }, [modal]);
    useEffect(() => {
        localStorage.setItem("maploom:favorites", JSON.stringify(favorites));
    }, [favorites]);
    useEffect(() => {
        const timer = setInterval(() => {
            if (current.current?.project_open) void run({type: "autosave"});
        }, 5 * 60 * 1000);
        return () => clearInterval(timer);
    }, [run, data?.project.id, data?.project_open]);
    useEffect(() => {
        if (!bridge.desktop) return;
        let disposed = false;
        let off: (() => void) | undefined;
        listen<string[]>("maploom-drop", (event) => {
            const project = event.payload.find((p) =>
                /\.(mloom|json|dungeondraft_map)$/i.test(p),
            );
            if (project) void run({type: "open", path: project});
            else {
                void run({type: "register_paths", paths: event.payload}, "Catalogs updated");
                setModal("catalogs");
            }
        }).then((fn) => {
            if (disposed) fn();
            else off = fn;
        });
        return () => {
            disposed = true;
            off?.();
        };
    }, [run]);
    useEffect(() => {
        if (!stage.current) return;
        const observer = new ResizeObserver(([e]) =>
            setViewport({w: e.contentRect.width, h: e.contentRect.height}),
        );
        observer.observe(stage.current);
        return () => observer.disconnect();
    }, [!!data, data?.project_open, right]);
    const fit = useCallback(() => {
        const p = current.current?.project;
        if (p) {
            setScale(
                Math.min((viewport.w - 96) / p.width, (viewport.h - 112) / p.height),
            );
            setPan({x: 0, y: 0});
        }
    }, [viewport]);
    useEffect(() => {
        fit();
    }, [data?.project.id, data?.project.width, data?.project.height, data?.project_open, viewport.w, viewport.h]);
    useEffect(() => {
        if (!data?.project_open) return;
        let alive = true;
        setRendering(true);
        setSceneError("");
        bridge.scene(level, grid, lights).then(
            (scene) => {
                if (alive && scene.revision === current.current?.revision) {
                    rememberTextBounds(current.current.project, scene.text_bounds ?? {});
                    setGpuScene(scene);
                }
            },
            (e) => {
                if (alive) {
                    setSceneError(String(e));
                    setRendering(false);
                }
            },
        );
        return () => {
            alive = false;
        };
    }, [data?.revision, data?.project.id, data?.project_open, level, grid, lights, sceneEpoch]);
    useEffect(() => {
        setPage(0);
    }, [query, category, favoriteOnly, assets.length]);
    useEffect(() => {
        setGesture([]);
        setPlacementCursor(null);
        if (toolCategory[tool]) {
            setCategory(tool === "room" ? materialTab : toolCategory[tool]!);
            setTab("library");
        }
    }, [tool, materialTab]);
    useEffect(() => {
        if (!data) return;
        setLevel((n) => Math.min(n, data.project.levels.length - 1));
        setLayer((n) =>
            Math.min(
                n,
                data.project.levels[Math.min(level, data.project.levels.length - 1)]
                    .layers.length - 1,
            ),
        );
    }, [data?.project.levels.length, level, data?.revision]);
    const project = data?.project,
        activeLevel = project?.levels[level],
        activeLayer = activeLevel?.layers[layer];
    useEffect(() => {
        const target = activeLevel?.layers.findIndex(l => l.name === defaultToolLayers[tool]);
        if (target !== undefined && target >= 0) setLayer(target);
        // Apply the preset only when entering a tool or map context, so manual
        // layer choices survive placements and other edits to the current map.
    }, [tool, activeLevel?.id, project?.id, project?.active_variation]);
    const allItems = activeLevel?.layers.flatMap((l) => l.items) ?? [],
        selection = allItems.filter((i) => selected.includes(i.id)),
        item = selection.length === 1 ? selection[0] : undefined;
    const prefabEntries = data?.prefabs ?? [];
    const activePrefab = prefabEntries.find(p => p.key === prefabKey) ?? prefabEntries[0];
    const canSavePrefab = canCreatePrefab(selection);
    const openPrefabDialog = () => {
        setPrefabName(t("New prefab"));
        setModal("prefab");
    };
    const activeGeometryMode = pathEditing ? item?.kind.type === "path" ? "points" : null : geometryMode;
    const editableItem =
        item &&
        !item.locked &&
        !activeLevel?.layers.find((l) => l.items.some((i) => i.id === item.id))
            ?.locked;
    const caveBlasts = useMemo(() => tool === "cave" && showBlasts && activeLevel
        ? activeLevel.layers.filter(l => l.visible).flatMap(l => l.items.flatMap(caveOpenedAreas)) : [], [tool, showBlasts, activeLevel]);
    useEffect(() => {
        setGeometryMode(null);
        setVertex(null);
        setGeometryDraft(null);
        setGesture([]);
    }, [item?.id, level, project?.id]);
    useEffect(() => {
        setPlacementCursor(null);
        setGesture([]);
        if (["scatter", "path", "room"].includes(dragRef.current?.mode ?? "")) {
            dragRef.current = null;
            setDrag(null);
        }
    }, [level, layer, project?.id, modal]);
    useEffect(() => {
        if (tool !== "select") {
            setGeometryMode(null);
            setVertex(null);
            setGeometryDraft(null);
        }
    }, [tool]);
    const selectedRoom = item?.kind.type === "room" ? item : undefined;
    const selectedPortal = item?.kind.type === "portal" ? item : undefined;
    const selectedPath = item?.kind.type === "path" && (tool === "select" || pathEditing) ? item : undefined;
    useEffect(() => {
        if (!wallSnap) return;
        for (const i of selection) if (i.kind.type === "object") {
            const meta = assetGeometry[i.kind.asset];
            if (meta) void bridge.assetSilhouette(meta).catch(() => {
            });
        }
    }, [wallSnap, selected, assetGeometry]);
    const roomPart =
        selectedRoom?.kind.type === "room"
            ? category === "tilesets"
                ? selectedRoom.kind.floor
                : category === "walls"
                    ? selectedRoom.kind.wall
                    : undefined
            : undefined;
    const available = assets.filter(
        (a) => (!a.builtin || project?.use_default_assets) && (a.category !== "roofs" || roofStyleAsset(a)),
    );
    const assetFor = (cat: string) => {
        const chosen = materials[cat];
        return chosen === null
            ? undefined
            : (available.find((a) => a.meta.id === chosen) ??
                available.find((a) => a.category === cat));
    };
    const activeAsset =
        selectedPortal && category === "portals" ? assets.find(a => a.meta.id === selectedPortal.texture) :
            selectedPath && category === "paths" ? assets.find(a => a.meta.id === selectedPath.texture) :
                item?.kind.type === "roof" && category === "roofs" ? assets.find(a => a.meta.id === item.texture) :
                    selectedRoom && ["tilesets", "walls"].includes(category)
                        ? assets.find((a) => a.meta.id === roomPart)
                        : assetFor(category);
    const [, setSilhouetteEpoch] = useState(0);
    useEffect(() => {
        let alive = true;
        if (wallSnap && tool === "object" && activeAsset)
            void bridge.assetSilhouette(activeAsset.meta).then(() => {
                if (alive) setSilhouetteEpoch(n => n + 1);
            }).catch(() => {
            });
        return () => {
            alive = false;
        };
    }, [wallSnap, tool, activeAsset?.meta.id, activeAsset?.meta.sha256]);
    const thumbnailColor = (asset: Asset) => {
        const floor = asset.category === "tilesets";
        const wall = asset.category === "walls";
        if (selectedRoom && (floor || wall))
            return roomTint(selectedRoom, floor);
        if (tool === "room" && (floor || wall))
            return rgba(floor ? floorColor : wallColor);
        if (item && (
            (floor && item.kind.type === "floor") ||
            (wall && item.kind.type === "wall") ||
            (asset.category === "portals" && item.kind.type === "portal") ||
            (asset.category === "paths" && !!selectedPath) ||
            (asset.category === "objects" && item.kind.type === "object")
        )) return item.tint;
        return rgba(color);
    };
    const changeAsset = (change: Partial<Asset["meta"]>) => {
        if (!activeAsset) return;
        void edit((p) => {
            p.assets[activeAsset.meta.id] = {...activeAsset.meta, ...change};
        });
    };
    const filtered = available.filter(
        (a) =>
            a.category === category &&
            (!favoriteOnly || favorites.includes(a.meta.id)) &&
            `${a.meta.name} ${a.meta.tags.join(" ")}`
                .toLowerCase()
                .includes(query.toLowerCase()),
    );
    const pageSize = 48,
        pages = Math.max(1, Math.ceil(filtered.length / pageSize));
    const chooseTool = (id: Tool) => {
        if (["scatter", "path", "room"].includes(dragRef.current?.mode ?? "")) {
            dragRef.current = null;
            setDrag(null);
        }
        setTool(id);
        if (id === "light") setLightTab("sources");
        setSelected([]);
        setGesture([]);
        setRoofCursor(null);
    };
    const changeItem = (change: (i: Item) => void) => {
        if (!item || !editableItem) return;
        void edit((p) => {
            const i = p.levels[level].layers
                .flatMap((l) => l.items)
                .find((i) => i.id === item.id);
            if (i) change(i);
        });
    };
    const chooseMaterial = (asset: Asset | null) => {
        setMaterials((m) => ({...m, [category]: asset?.meta.id ?? null}));
        if (selectedPortal && category === "portals" && asset) {
            changeItem(i => setPortalStyle(i, asset.meta, assets.find(a => a.meta.id === i.texture)?.meta));
            return;
        }
        if (selectedPath && category === "paths" && asset) {
            changeItem(i => setPathTexture(i, asset.meta, assets.find(a => a.meta.id === i.texture)?.meta));
            return;
        }
        if (item?.kind.type === "roof" && category === "roofs" && asset) {
            changeItem(i => {
                if (i.kind.type === "roof") {
                    i.texture = asset.meta.id;
                    Object.assign(i.kind, roofStyle(asset, assets));
                }
            });
            return;
        }
        if (!selectedRoom || !["tilesets", "walls"].includes(category)) return;
        changeItem((i) => {
            if (i.kind.type !== "room") return;
            if (category === "tilesets") {
                i.kind.floor = asset?.meta.id ?? null;
                if (asset) i.texture_scale = asset.meta.size.x * floorScale;
            } else {
                i.kind.wall = asset?.meta.id ?? null;
            }
        });
    };
    const browseRoomMaterial = (part: "tilesets" | "walls") => {
        setMaterialTab(part);
        setCategory(part);
        setQuery("");
        setFavoriteOnly(false);
        setTab("library");
        setRight(true);
    };
    const changePathMode = (mode: typeof pathMode) => {
        updateDrag(null);
        setGeometryDraft(null);
        setGeometryMode(null);
        setVertex(null);
        setGesture([]);
        setRoofCursor(null);
        setPathMode(mode);
        if (mode === "edit") {
            if (item?.kind.type !== "path") setSelected([]);
            setTab("inspector");
            setRight(true);
        } else setSelected([]);
    };
    const browsePathTexture = () => {
        if (tool === "path" && !pathEditing) changePathMode("edit");
        else if (tool !== "select" && !pathEditing) setTool("select");
        setCategory("paths");
        setQuery("");
        setFavoriteOnly(false);
        setPage(0);
        setTab("library");
        setRight(true);
    };
    const deleteSelection = () =>
        void run({type: "delete", level, ids: selected}).then(() =>
            setSelected([]),
        );
    const duplicateSelection = () =>
        void run({type: "duplicate", level, ids: selected});
    const newDialog = () => {
        setModal("new");
        setFileMenu(false);
    };
    const preparePlacement = (i: Item) => {
        // Rooms already carry independent floor/wall colors; a shared tint would
        // multiply both by the unrelated color last used by another tool.
        if (i.kind.type !== "room" && i.kind.type !== "light" && i.kind.type !== "text") i.tint = rgba(color);
        if (
            i.kind.type === "object" &&
            assetFor("objects")?.meta.color_mode === "none"
        )
            i.tint = white;
        if (i.texture) {
            const a = assets.find((a) => a.meta.id === i.texture);
            if (a) i.texture_scale = i.kind.type === "path" ? a.meta.size.x / Math.max(0.01, a.meta.size.y) * i.kind.width * pathScale
                : a.meta.size.x * (i.kind.type === "floor" || i.kind.type === "roof" ? floorScale : 1);
        }
        return i;
    };
    const add = (i: Item) => {
        preparePlacement(i);
        if (i.kind.type === "roof" && roofUnder || i.kind.type === "path" && pathUnder) {
            void edit(p => {
                const l = p.levels[level].layers[layer];
                if (!l.locked && l.visible) l.items.unshift(i);
            }).then(() => setSelected([i.id]));
            return;
        }
        void run({type: "add", level, layer, item: i}).then(() =>
            setSelected([i.id]),
        );
    };
    const commitGeometry = (next: Item) => {
        if (!validGeometry(next)) {
            setToast("Invalid geometry");
            return;
        }
        // A dragged hole must remain a hole, including when its contour flips winding.
        if (item?.kind.type === "room" && next.kind.type === "room" && item.kind.rings.length === next.kind.rings.length &&
            item.kind.rings.some((r, n) => (signedArea(r) > 0) !== (signedArea((next.kind as Extract<Item["kind"], {
                type: "room"
            }>).rings[n]) > 0))) {
            setToast("Invalid geometry");
            return;
        }
        changeItem(i => {
            i.kind = next.kind;
            i.floor_holes = next.floor_holes;
            i.wall_exclusions = next.wall_exclusions;
        });
    };
    const architectureItem = (ps: Point[], knownContour = false): Item | null => {
        if (tool === "path") {
            const asset = assetFor("paths");
            if (!asset) throw Error("Choose asset");
            const i = newItem(t("Path"), {
                type: "path",
                points: ps,
                width: pathWidth,
                color: rgba(color)
            }, asset.meta.id);
            i.path_style = {...pathStyle};
            if (!validGeometry(i)) throw Error("Invalid geometry");
            return i;
        }
        if (tool === "roof") {
            const asset = assetFor("roofs");
            if (!asset) throw Error("Choose asset");
            const shape = roofMode === "quickbox" ? roofQuickbox(ps[0], ps[2]) : {points: ps, width: roofWidth};
            const k = newRoof(shape.points, shape.width, roofType, asset, assets);
            Object.assign(k, {shade: roofShade, sun_direction: roofSun, contrast: roofContrast});
            const i = newItem(t("Roof"), k, asset.meta.id);
            if (!validGeometry(i)) throw Error("Invalid geometry");
            return i;
        }
        const floor = assetFor("tilesets"), wall = assetFor("walls");
        if ((tool === "room" && newRoomWalls && !wall) || (tool === "wall" && !wall) || (tool === "floor" && !floor)) {
            throw Error("Choose asset");
        }
        let i: Item;
        if (tool === "room") {
            i = newItem(t("Room"), {
                type: "room",
                rings: [oriented(ps, true)],
                floor: floor?.meta.id ?? null,
                wall: wall?.meta.id ?? null,
                width
            });
            i.texture_scale = (floor?.meta.size.x ?? 1) * floorScale;
            i.floor_tint = rgba(floorColor);
            i.wall_tint = rgba(wallColor);
            if (!newRoomWalls) i.wall_exclusions = [0];
        } else if (tool === "floor") i = newItem(t("Floor"), {
            type: "floor",
            points: oriented(ps, true),
            color: rgba(color)
        }, floor!.meta.id);
        else if (tool === "wall") i = newItem(t("Wall"), {
            type: "wall",
            points: ps,
            width,
            color: rgba(color),
            blocks_sight: true
        }, wall!.meta.id);
        else return null;
        if (!knownContour && !validGeometry(i)) {
            throw Error("Invalid geometry");
        }
        return i;
    };
    const createArchitecture = (ps: Point[]) => {
        try {
            const i = architectureItem(ps);
            if (i) add(i);
        } catch (error) {
            setToast((error as Error).message);
        }
    };
    const finishHole = (ps: Point[]) => {
        if (!item || !editableItem || ps.length < 3) return;
        const next = structuredClone(item);
        addHole(next, ps, holeWalls);
        if (!validGeometry(next)) {
            setToast("Invalid geometry");
            return;
        }
        commitGeometry(next);
        setGesture([]);
        setGeometryMode("points");
    };
    const finishShape = () => {
        if (pathEditing) return;
        if (geometryMode === "hole") {
            finishHole(gesture.filter((p, n) => n === 0 || !samePoint(p, gesture[n - 1])));
            return;
        }
        if (!activeLayer || activeLayer.locked || !activeLayer.visible) {
            setGesture([]);
            return;
        }
        const ps = gesture.filter(
            (p, i) => i === 0 || p.x !== gesture[i - 1].x || p.y !== gesture[i - 1].y,
        );
        if (tool === "terrain" && terrainMode === "water" && waterShape === "polygon") {
            if (ps.length < 3) return;
            void edit(p => {
                const l = p.levels[level].layers[layer];
                if (paintWaterOnLevel(p.levels[level], l, [], radius, water, waterShape, t("Water"), ps) === null) setToast("Locked");
            });
            setGesture([]);
            return;
        }
        if (["room", "floor", "wall", "roof", "path"].includes(tool) && ps.length >= (tool === "wall" || tool === "roof" || tool === "path" ? 2 : 3)) createArchitecture(ps);
        setGesture([]);
    };
    const commands = [
        ...tools.map((o) => ({
            label: o.label,
            key: o.key,
            icon: o.icon,
            action: () => chooseTool(o.id),
        })),
        {label: "New", key: "Ctrl N", icon: FilePlus2, action: newDialog},
        {
            label: "Open",
            key: "Ctrl O",
            icon: FolderOpen,
            action: () => void run({type: "open", path: null}),
        },
        {
            label: "Save",
            key: "Ctrl S",
            icon: Save,
            action: () => void run({type: "save", save_as: false}, "Saved toast"),
        },
        {
            label: "Export",
            key: "Ctrl Shift E",
            icon: Download,
            action: () => setModal("export"),
        },
        {label: "Fit", key: "F", icon: Maximize, action: fit},
        {
            label: "Undo",
            key: "Ctrl Z",
            icon: Undo2,
            action: () => void run({type: "undo"}),
        },
        {
            label: "Redo",
            key: "Ctrl Shift Z",
            icon: Redo2,
            action: () => void run({type: "redo"}),
        },
        {
            label: "Settings",
            key: ",",
            icon: Settings2,
            action: () => setModal("settings"),
        },
        {
            label: "Recover",
            key: "",
            icon: History,
            action: () => void run({type: "recover"}),
        },
    ];
    useEffect(() => {
        const down = (e: KeyboardEvent) => {
            if (updates.installing) {
                e.preventDefault();
                return;
            }
            const input = (e.target as HTMLElement).closest(
                "input,textarea,select,[contenteditable]",
            );
            if (e.key === "Escape") {
                setModal(null);
                setFileMenu(false);
                setGesture([]);
                dragRef.current = null;
                setDrag(null);
                setGeometryDraft(null);
                if (geometryMode === "hole") setGeometryMode("points");
                return;
            }
            const ctrl = e.ctrlKey || e.metaKey;
            const k = shortcutKey(e);
            if (!data?.project_open) {
                if (input || e.isComposing || e.altKey || modal) return;
                if (ctrl && (k === "n" || k === "o")) {
                    e.preventDefault();
                    if (k === "n") newDialog();
                    else void run({type: "open", path: null});
                } else if (e.key === ",") setModal("settings");
                return;
            }
            // Saving is a document command even while editing an inspector field.
            if (ctrl && !e.altKey && k === "s" && !modal && !e.isComposing) {
                e.preventDefault();
                // Inspector fields commit on blur; enqueue that edit before saving.
                if (input instanceof HTMLElement) input.blur();
                void run({type: "save", save_as: e.shiftKey}, "Saved toast");
                return;
            }
            const textInput = input && !input.matches('input[type="number"],input[type="range"],input[type="color"],input[type="checkbox"],select');
            if (ctrl && !e.altKey && !modal && !e.isComposing && !textInput && (k === "z" || k === "y")) {
                e.preventDefault();
                if (input instanceof HTMLElement) input.blur();
                void run({type: k === "y" || e.shiftKey ? "redo" : "undo"});
                return;
            }
            if (input || e.isComposing || e.altKey) return;
            if (e.code === "Space") {
                e.preventDefault();
                space.current = true;
                setPlacementCursor(null);
            }
            if (ctrl && k === "k") {
                e.preventDefault();
                setCommandQuery("");
                setModal((m) => (m === "commands" ? null : "commands"));
                return;
            }
            if (modal) return;
            if (ctrl) {
                if (["s", "o", "n", "z", "y", "d", "e"].includes(k)) e.preventDefault();
                if (k === "o") void run({type: "open", path: null});
                if (k === "n") newDialog();
                if (k === "d") duplicateSelection();
                if (k === "e" && e.shiftKey) setModal("export");
            } else {
                if (e.key === "Enter") finishShape();
                if (e.key === "Delete" || e.key === "Backspace") {
                    e.preventDefault();
                    if (activeGeometryMode === "points" && vertex && item) {
                        const next = structuredClone(item);
                        if (removeVertex(next, vertex)) {
                            commitGeometry(next);
                            setVertex(null);
                        }
                    } else if (gesture.length) setGesture(ps => ps.slice(0, -1));
                    else deleteSelection();
                }
                if (k === "f") fit();
                if (e.key === ",") setModal("settings");
                const chosen = tools.find(
                    (o) => o.key.toLowerCase() === k,
                );
                if (chosen) chooseTool(chosen.id);
            }
        };
        const up = (e: KeyboardEvent) => {
            if (e.code === "Space") space.current = false;
        };
        const blur = () => {
            space.current = false;
            setPlacementCursor(null);
            if ((dragRef.current?.mode === "scatter" || dragRef.current?.mode === "path")) {
                dragRef.current = null;
                setDrag(null);
            }
        };
        window.addEventListener("keydown", down);
        window.addEventListener("keyup", up);
        window.addEventListener("blur", blur);
        return () => {
            window.removeEventListener("keydown", down);
            window.removeEventListener("keyup", up);
            window.removeEventListener("blur", blur);
        };
    });
    useEffect(() => {
        const node = stage.current;
        if (!node) return;
        const wheel = (e: WheelEvent) => handleWheel(e);
        node.addEventListener("wheel", wheel, {passive: false});
        return () => node.removeEventListener("wheel", wheel);
    });
    const dark = data ? themes.mode === "dark" : bridge.startupDark;
    if (!data || !project || !activeLevel || !activeLayer)
        return (
            <div className={`loading-screen ${dark ? "dark" : "light"}`}>
                <img src={dark ? logoDark : logo} alt="Maploom"/>
                {error ? (
                    <>
                        <p>{error}</p>
                        <button onClick={() => location.reload()}>{t("Retry")}</button>
                    </>
                ) : (
                    <LoaderCircle className="spin"/>
                )}
            </div>
        );
    const origin = {
        x: (viewport.w - project.width * scale) / 2 + pan.x,
        y: (viewport.h - project.height * scale) / 2 + pan.y,
    };
    const world = (
        e: { clientX: number; clientY: number },
        snapped = true,
    ): Point => {
        const rect = stage.current!.getBoundingClientRect();
        let x = (e.clientX - rect.left - origin.x) / scale,
            y = (e.clientY - rect.top - origin.y) / scale;
        if (snapped && snap) {
            x = Math.round(x * 2) / 2;
            y = Math.round(y * 2) / 2;
        }
        return {x, y};
    };
    const inMap = (p: Point) =>
        p.x >= 0 && p.x <= project.width && p.y >= 0 && p.y <= project.height;
    const updateDrag = (d: Drag | null) => {
        dragRef.current = d;
        setDrag(d ? {...d, trail: [...d.trail]} : null);
    };
    const movePosition = (d: Drag, raw: Point, fallback: Point): Point => {
        if (!wallSnap) return fallback;
        const moving = activeLevel.layers.filter(l => l.visible && !l.locked).flatMap(l => l.items)
            .filter(i => !i.locked && d.ids.includes(i.id));
        const hulls = new Map<string, Point[]>();
        for (const i of moving) if (i.kind.type === "object") {
            const meta = assetGeometry[i.kind.asset], hull = meta && bridge.cachedSilhouette(meta);
            if (hull) hulls.set(i.kind.asset, hull);
        }
        const anchor = d.rawStart ?? d.start;
        const delta = snapObjectsToWalls(moving, {x: raw.x - anchor.x, y: raw.y - anchor.y},
            snapWalls(activeLevel, new Set(d.ids)), hulls, project.pixels_per_grid, 12 / scale);
        return delta ? {x: d.start.x + delta.x, y: d.start.y + delta.y} : fallback;
    };
    const trackPlacement = (e: ReactPointerEvent) => {
        if (!["object", "prefab", "portal", "scatter", "room", "floor", "wall", "path", "roof"].includes(tool)) return;
        const target = e.target as HTMLElement;
        setPlacementCursor(
            space.current || dragRef.current?.mode === "pan" || target.closest("button")
                ? null : {x: e.clientX, y: e.clientY},
        );
    };
    const placementAsset = tool === "object" ? assetFor("objects") : undefined;
    const handleWheel = (e: WheelEvent) => {
        if ((e.target as HTMLElement).closest("button,input,select,textarea")) return;
        e.preventDefault();
        if (!e.deltaY || modal) return;
        if (!e.ctrlKey && !e.metaKey && !space.current && !dragRef.current) {
            const degrees = Math.sign(e.deltaY) * (e.shiftKey ? 1 : 15);
            if ((placementAsset || tool === "prefab") && activeLayer.visible && !activeLayer.locked) {
                setRotation(value => ((value + degrees) % 360 + 360) % 360);
                setPlacementCursor({x: e.clientX, y: e.clientY});
                return;
            }
            if (tool === "select" && !geometryMode && activeLevel.layers.some(l =>
                l.visible && !l.locked && l.items.some(i => selected.includes(i.id) && !i.locked && i.kind.type === "object"))) {
                const ids = [...selected];
                void edit(p => {
                    for (const l of p.levels[level].layers.filter(l => l.visible && !l.locked)) {
                        for (const i of l.items) {
                            if (!i.locked && ids.includes(i.id) && i.kind.type === "object") {
                                const angle = i.kind.rotation + degrees * Math.PI / 180;
                                i.kind.rotation = ((angle % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);
                            }
                        }
                    }
                });
                return;
            }
        }
        const p = world(e, false);
        const next = Math.max(2, Math.min(400, scale * Math.exp(-e.deltaY * 0.0015)));
        setPan(v => ({
            x: v.x + (project.width / 2 - p.x) * (next - scale),
            y: v.y + (project.height / 2 - p.y) * (next - scale),
        }));
        setScale(next);
    };
    const snapPlacementPoint = (a: Asset, raw: Point, fallback: Point): Point => {
        const hull = wallSnap && bridge.cachedSilhouette(a.meta);
        if (!hull) return fallback;
        const i = newItem(a.meta.name, objectPlacement(a.meta, raw, objectScale, rotation));
        const delta = snapObjectsToWalls([i], {x: 0, y: 0}, snapWalls(activeLevel),
            new Map([[a.meta.id, hull]]), project.pixels_per_grid, 12 / scale);
        return delta ? {x: raw.x + delta.x, y: raw.y + delta.y} : fallback;
    };
    const placementEvent = placementCursor ? {clientX: placementCursor.x, clientY: placementCursor.y} : null;
    const placementPoint = placementEvent ? placementAsset
        ? snapPlacementPoint(placementAsset, world(placementEvent, false), world(placementEvent))
        : world(placementEvent, tool !== "scatter") : null;
    const placeObject = async (a: Asset, raw: Point, fallback: Point) => {
        const revision = data.revision;
        try {
            if (wallSnap) await bridge.assetSilhouette(a.meta);
            if (current.current?.project.id !== project.id || current.current.revision !== revision) {
                setToast("Placement changed");
                return;
            }
            const placed = newItem(a.meta.name, objectPlacement(a.meta, snapPlacementPoint(a, raw, fallback), objectScale, rotation));
            placed.blocks_light = objectBlocksLight;
            add(placed);
        } catch {
            setToast("Silhouette unavailable");
        }
    };
    const showPlacement = placementPoint && inMap(placementPoint) && !modal && !fileMenu && !busy &&
        !activeLayer.locked && activeLayer.visible && drag?.mode !== "pan";
    const portalAsset = tool === "portal" ? assetFor("portals") : undefined;
    const portalRequest: bridge.PortalPreviewRequest | null = portalAsset && showPlacement && placementPoint ? {
        level, layer, position: placementPoint, asset: portalAsset.meta.id,
        color: rgba(color), grid, lights, revision: data.revision,
    } : null;
    const portalPreviewScene = portalRequest && portalScene?.scope === portalPreviewScope(portalRequest) ? portalScene.scene : null;
    const drawingTool = !pathEditing && ["room", "floor", "wall", "path", "roof"].includes(tool);
    const tentativePoints = drawingTool ? drawingPoints(tool, drawShape, gesture,
        showPlacement ? placementPoint : null, drag, segmentation, scale) : [];
    let drawingRequest: bridge.DrawingPreviewRequest | null = null;
    if (drawingTool && !modal && !fileMenu && !busy && activeLayer.visible && !activeLayer.locked &&
        tentativePoints.length >= (tool === "wall" || tool === "path" || tool === "roof" ? 2 : 3)) {
        try {
            // Rectangles and regular circles are simple by construction. Avoid
            // quadratic intersection checks on every frame of a 512-point circle.
            const knownContour = drag?.mode === "room" && (tool === "room" || tool === "floor");
            if (knownContour && (!tentativePoints.every(p => Number.isFinite(p.x) && Number.isFinite(p.y)) ||
                Math.abs(signedArea(tentativePoints)) < 1e-7)) throw Error("Invalid geometry");
            const i = architectureItem(tentativePoints, knownContour);
            if (i) {
                i.id = "__drawing_preview__";
                drawingRequest = {
                    level, layer, item: preparePlacement(i),
                    under: tool === "path" && pathUnder || tool === "roof" && roofUnder,
                    grid, lights, revision: data.revision
                };
            }
        } catch { /* Incomplete/invalid geometry keeps its editable contour. */
        }
    }
    // A gesture's first point is stable while the cursor moves. Include all
    // material settings so an old frame cannot override a new style or tool.
    const drawingScope = JSON.stringify([project.id, data.revision, level, layer, tool,
        drawShape, drag?.start ?? gesture[0], drawingRequest && {
            ...drawingRequest,
            item: {
                ...drawingRequest.item, kind: {
                    ...drawingRequest.item.kind, points: undefined, rings: undefined,
                    ...(tool === "roof" && roofMode === "quickbox" ? {width: undefined} : {})
                }
            }
        }]);
    const drawingPreviewScene = drawingRequest && drawingScene?.scope === drawingScope ? drawingScene.scene : null;
    const transientScene = portalPreviewScene ?? drawingPreviewScene;
    const pickVisible = (point: Point, layerId?: string, type?: Item["kind"]["type"]) => {
        const items = activeLevel.layers.filter(l => l.visible && (layerId === undefined || l.id === layerId))
            .flatMap(l => l.items).filter(i => type === undefined || i.kind.type === type);
        const id = picker.current?.pick(point, scale, new Set(items.map(i => i.id)));
        return items.find(i => i.id === id);
    };
    const pointerDown = (e: ReactPointerEvent) => {
        if (e.button === 2) return;
        if (busy && tool !== "pan" && e.button !== 1 && !space.current) return;
        trackPlacement(e);
        const pendingText = document.activeElement instanceof HTMLTextAreaElement && document.activeElement.closest(".text-editor")
            ? document.activeElement.value : undefined;
        if (document.activeElement instanceof HTMLTextAreaElement) document.activeElement.blur();
        e.currentTarget.setPointerCapture(e.pointerId);
        const p = world(e, !(tool === "cave" || tool === "terrain" && terrainMode === "water" && waterShape !== "polygon"));
        if (tool === "roof" || tool === "path") setRoofCursor(p);
        const screen = {x: e.clientX, y: e.clientY};
        if (tool === "pan" || e.button === 1 || space.current) {
            setPlacementCursor(null);
            updateDrag({
                start: p,
                last: p,
                screen,
                mode: "pan",
                ids: [],
                trail: [],
                additive: false,
            });
            return;
        }
        if (!inMap(p)) return;
        if ((tool === "select" || pathEditing) && activeGeometryMode && item && editableItem) {
            const raw = world(e, false);
            const startDrag = (mode: "vertex" | "resize" | "hole", source?: Item, v?: Vertex, corner?: number) =>
                updateDrag({
                    start: p,
                    last: mode === "vertex" && source && v ? {...geometryRings(source)[v.ring][v.point]} : p,
                    screen,
                    mode,
                    ids: [item.id],
                    trail: [],
                    additive: false,
                    source,
                    vertex: v,
                    corner
                });
            if (activeGeometryMode === "hole") {
                if (holeShape === "polygon") {
                    if (gesture.length >= 3 && Math.hypot(raw.x - gesture[0].x, raw.y - gesture[0].y) < 8 / scale) finishShape();
                    else setGesture(ps => [...ps, p]);
                } else startDrag("hole");
                return;
            }
            if (activeGeometryMode === "resize") {
                const b = bounds(item), corners = rectangle({x: b.x, y: b.y}, {x: b.right, y: b.bottom});
                const n = corners.findIndex(q => Math.hypot(q.x - raw.x, q.y - raw.y) < 9 / scale);
                if (n >= 0) {
                    startDrag("resize", structuredClone(item), undefined, n);
                    return;
                }
            } else {
                const chosen = geometryTarget(item, raw, scale);
                if (chosen) {
                    const source = structuredClone(item);
                    if (chosen.type === "edge") insertVertex(source, chosen, chosen.position);
                    setVertex({ring: chosen.ring, point: chosen.point});
                    setGeometryDraft(source);
                    startDrag("vertex", source, chosen);
                    // A click inserts exactly on the edge, even with grid snapping.
                    // Grid snapping applies when the new or existing point is dragged.
                }
                if (chosen || !pathEditing) return;
            }
        }
        if (pathEditing) {
            const path = pickVisible(world(e, false), undefined, "path");
            setSelected(path ? [path.id] : []);
            setVertex(null);
            if (path) {
                setLayer(activeLevel.layers.findIndex(l => l.items.some(i => i.id === path.id)));
                setTab("inspector");
                setRight(true);
                updateDrag({
                    start: p,
                    last: p,
                    screen,
                    mode: "move",
                    rawStart: world(e, false),
                    ids: [path.id],
                    trail: [],
                    additive: false
                });
            }
            return;
        }
        const hit = pickVisible(world(e, false), tool === "select" || tool === "erase" ? activeLayer.id : undefined);
        if (tool === "text") {
            if (hit?.kind.type === "text") {
                setSelected([hit.id]);
                setLayer(activeLevel.layers.findIndex(l => l.items.some(i => i.id === hit.id)));
                if (textMode === "move") updateDrag({
                    start: p,
                    last: p,
                    screen,
                    mode: "move",
                    rawStart: world(e, false),
                    ids: [hit.id],
                    trail: [],
                    additive: false
                });
                else requestAnimationFrame(() => stage.current?.closest(".workspace")?.querySelector<HTMLTextAreaElement>(".tool-controls .text-editor textarea")?.focus());
                return;
            }
            if (textMode === "move") {
                setSelected([]);
                return;
            }
        }
        let ids = selected;
        if (tool === "select") {
            ids = selected.filter(id => activeLayer.items.some(i => i.id === id));
            if (hit) {
                ids = e.shiftKey
                    ? ids.includes(hit.id)
                        ? ids.filter((id) => id !== hit.id)
                        : [...ids, hit.id]
                    : ids.includes(hit.id)
                        ? ids
                        : [hit.id];
                setSelected(ids);
                setTab("inspector");
                if (wallSnap) for (const i of allItems) if (ids.includes(i.id) && i.kind.type === "object") {
                    const meta = assetGeometry[i.kind.asset];
                    if (meta) void bridge.assetSilhouette(meta).catch(() => {
                    });
                }
                updateDrag({
                    start: p,
                    last: p,
                    screen,
                    mode: "move",
                    rawStart: world(e, false),
                    ids,
                    trail: [],
                    additive: e.shiftKey,
                });
            } else {
                setSelected(e.shiftKey ? ids : []);
                updateDrag({
                    start: p,
                    last: p,
                    screen,
                    mode: "box",
                    ids,
                    trail: [],
                    additive: e.shiftKey,
                });
            }
            return;
        }
        if (activeLayer.locked || !activeLayer.visible) {
            setToast("Locked");
            return;
        }
        if (tool === "prefab") {
            if (!activePrefab) {
                setToast("Choose prefab");
                return;
            }
            void run({
                type: "place_prefab", key: activePrefab.key, level, layer, position: p,
                scale: objectScale, rotation: rotation * Math.PI / 180
            });
            return;
        }
        if (tool === "scatter") {
            if (!nextScatter) {
                setToast("Scatter choose assets");
                return;
            }
            const position = world(e, false);
            if (!inMap(position)) return;
            const first = structuredClone(nextScatter);
            if (first.kind.type === "object") first.kind.position = position;
            setSelected([]);
            updateDrag({
                start: position, last: position, screen, mode: "scatter", ids: [], trail: [], additive: false,
                scatter: {last: position, remaining: scatterSettings.spread}, items: [first], revision: data.revision
            });
            return;
        }
        if (tool === "terrain" && terrainMode === "water" && waterShape === "polygon") {
            setGesture(ps => [...ps, p]);
            return;
        }
        if (tool === "path") {
            if (!assetFor("paths")) {
                setToast("Choose asset");
                return;
            }
            if (pathMode === "points") setGesture(ps => ps.length && samePoint(ps.at(-1)!, p) ? ps : [...ps, p]);
            else {
                const raw = world(e, false);
                if (inMap(raw)) updateDrag({
                    start: raw,
                    last: raw,
                    screen,
                    mode: "path",
                    ids: [],
                    trail: [raw],
                    additive: false
                });
            }
            return;
        }
        if (["room", "wall", "floor", "roof"].includes(tool) && drawShape === "polygon") {
            if (gesture.length >= 3 && tool !== "wall" && tool !== "roof" && Math.hypot(world(e, false).x - gesture[0].x, world(e, false).y - gesture[0].y) < 8 / scale) finishShape();
            else setGesture(ps => [...ps, p]);
            return;
        }
        if (["room", "wall", "floor", "roof"].includes(tool) || tool === "terrain" || tool === "cave" || tool === "erase") {
            updateDrag({
                start: p,
                last: p,
                screen,
                mode: ["room", "wall", "floor", "roof"].includes(tool) ? "room" : tool === "cave" ? "terrain" : tool as "terrain" | "erase",
                ids: hit ? [hit.id] : [],
                trail: [p],
                additive: false,
            });
            return;
        }
        if (tool === "object") {
            const a = assetFor("objects");
            if (!a) {
                setToast("Choose asset");
                return;
            }
            void placeObject(a, world(e, false), p);
        }
        if (tool === "portal") {
            const a = assetFor("portals");
            if (!a) {
                setToast("Choose asset");
                return;
            }
            void run({
                type: "portal",
                level,
                layer,
                position: p,
                asset: a.meta.id,
                color: rgba(color),
            });
        }
        if (tool === "light" && lightTab === "sources")
            add(
                newItem(t("Light"), {
                    type: "light",
                    position: p,
                    ...light,
                    shadows: true,
                }),
            );
        if (tool === "text") {
            const source = item?.kind.type === "text" ? item.kind : textDraft;
            const next = newItem(t("Text"), {...source, position: p, text: (pendingText ?? source.text) || t("Text")});
            next.text_style = structuredClone(item?.kind.type === "text" ? item.text_style ?? defaultTextStyle() : textStyle);
            add(next);
        }
    };
    const pointerMove = (e: ReactPointerEvent) => {
        trackPlacement(e);
        const d = dragRef.current;
        const p = world(e, !(tool === "cave" || tool === "terrain" && terrainMode === "water" && waterShape !== "polygon"));
        if (tool === "roof" || tool === "path") setRoofCursor(p);
        if (!d && (tool === "select" || pathEditing) && activeGeometryMode === "points" && item && editableItem)
            setGeometryHover(geometryTarget(item, world(e, false), scale));
        if (tool === "cave" || tool === "terrain" && terrainMode === "water" && waterShape !== "polygon")
            setBrushCursor(inMap(p) ? p : null);
        if (!d) return;
        if (d.mode === "path") {
            const raw = world(e, false);
            if (inMap(raw) && Math.hypot(raw.x - d.last.x, raw.y - d.last.y) >= 2 / scale) {
                d.last = raw;
                d.trail.push(raw);
                updateDrag(d);
            }
            return;
        }
        if (d.mode === "scatter" && d.scatter && d.items) {
            const raw = world(e, false);
            // Do not fill the off-map path when the pointer leaves and re-enters.
            if (!inMap(raw)) {
                d.scatter.last = raw;
                d.scatter.remaining = scatterSettings.spread;
            } else {
                const positions = extendScatterStroke(d.scatter, raw, scatterSettings.spread).filter(inMap);
                for (const position of positions) {
                    const next = scatterItem(scatterAssets, position, {...scatterSettings, color: rgba(color)});
                    if (next) d.items.push(next);
                }
            }
            d.last = raw;
            updateDrag(d);
            return;
        }
        if (d.mode === "pan") {
            // Capture the delta before mutating drag state: React can defer this updater.
            const dx = e.clientX - d.screen.x,
                dy = e.clientY - d.screen.y;
            setPan((v) => ({
                x: v.x + dx,
                y: v.y + dy,
            }));
            d.screen = {x: e.clientX, y: e.clientY};
        } else {
            d.last = p;
            if (d.mode === "move") {
                d.rawLast = world(e, false);
                d.last = movePosition(d, d.rawLast, p);
            }
            if (d.mode === "terrain" && inMap(p))
                appendTerrainPoint(d.trail, p, radius);
            if (d.mode === "erase") {
                const hit = pickVisible(world(e, false), activeLayer.id);
                if (hit && !d.ids.includes(hit.id)) d.ids.push(hit.id);
            }
        }
        if (d.source && (d.mode === "vertex" || d.mode === "resize")) {
            const next = structuredClone(d.source);
            if (d.mode === "vertex" && d.vertex) {
                const position = Math.hypot(e.clientX - d.screen.x, e.clientY - d.screen.y) >= 3
                    ? p : geometryRings(d.source)[d.vertex.ring][d.vertex.point];
                d.last = {...position};
                moveVertex(next, d.vertex, position);
            }
            if (d.mode === "resize" && d.corner !== undefined) resizeFromCorner(next, d.corner, p);
            setGeometryDraft(next);
        }
        updateDrag(d);
    };
    const pointerUp = async (e: ReactPointerEvent) => {
        const d = dragRef.current;
        if (!d) return;
        if (d.mode === "scatter" || d.mode === "path") pointerMove(e);
        if (d.mode === "path") {
            updateDrag(null);
            const raw = world(e, false);
            if (inMap(raw) && !samePoint(raw, d.trail.at(-1)!)) d.trail.push(raw);
            const ps = simplifyPath(d.trail, 1.5 / scale);
            if (ps.length >= 2) createArchitecture(ps);
            return;
        }
        if (d.mode === "move" && wallSnap && d.rawLast) {
            const revision = data.revision;
            try {
                await Promise.all(allItems.filter(i => d.ids.includes(i.id)).map(i => {
                    const meta = i.kind.type === "object" && assetGeometry[i.kind.asset];
                    return meta ? bridge.assetSilhouette(meta) : Promise.resolve([]);
                }));
                if (dragRef.current !== d) return;
                if (current.current?.project.id !== project.id || current.current.revision !== revision) {
                    updateDrag(null);
                    setToast("Placement changed");
                    return;
                }
                d.last = movePosition(d, d.rawLast, d.last);
            } catch {
                updateDrag(null);
                setToast("Silhouette unavailable");
                return;
            }
        }
        updateDrag(null);
        if (d.mode === "scatter") {
            setScatterEpoch(n => n + 1);
            if (current.current?.project.id !== project.id || current.current.revision !== d.revision) {
                setToast("Placement changed");
                return;
            }
            if (d.items?.length) {
                const items = d.items;
                void edit(p => {
                    const target = p.levels[level].layers[layer];
                    if (!target.locked && target.visible) target.items.push(...items);
                });
            }
            return;
        }
        const a = d.start,
            b = d.last;
        if (d.mode === "vertex" || d.mode === "resize") {
            if (d.source) {
                const next = structuredClone(d.source);
                if (d.mode === "vertex" && d.vertex) moveVertex(next, d.vertex, d.last);
                if (d.mode === "resize" && d.corner !== undefined) resizeFromCorner(next, d.corner, d.last);
                commitGeometry(next);
            }
            setGeometryDraft(null);
            return;
        }
        if (d.mode === "hole") {
            finishHole(holeShape === "circle" ? circleContour(a, b, segmentation) : rectangle(a, b));
            return;
        }
        if (d.mode === "room" && Math.hypot(a.x - b.x, a.y - b.y) > 0.1) {
            let ps = drawShape === "circle" ? circleContour(a, b, segmentation) : rectangle(a, b);
            if (tool === "wall") ps = [...ps, {...ps[0]}];
            createArchitecture(ps);
        }
        if (d.mode === "move" && (a.x !== b.x || a.y !== b.y))
            void run({
                type: "move",
                level,
                ids: d.ids,
                delta: {x: b.x - a.x, y: b.y - a.y},
            });
        if (d.mode === "box") {
            const ids = boxSelection(activeLayer, a, b, assetGeometry, d.additive ? d.ids : []);
            setSelected(ids);
            if (ids.length) setTab("inspector");
        }
        if (d.mode === "erase")
            void run({type: "delete", level, ids: d.ids}).then(() =>
                setSelected([]),
            );
        if (d.mode === "terrain") {
            if (tool === "cave") {
                if (caveMode === "dig" && !activeCaveMaterial) {
                    setToast("No cave styles");
                    return;
                }
                void edit(p => {
                    if (paintCave(p.levels[level], p.levels[level].layers[layer], d.trail, radius, caveMode, cave,
                        activeCaveMaterial, t("Cave")) === null) setToast("Locked");
                });
                return;
            }
            if (terrainMode === "water") {
                void edit(p => {
                    const l = p.levels[level].layers[layer];
                    if (paintWaterOnLevel(p.levels[level], l, d.trail, radius, water, waterShape, t("Water")) === null) setToast("Locked");
                });
                return;
            }
            const asset = assetFor("terrain");
            if (!asset) {
                setToast("Choose asset");
                return;
            }
            void edit((p) => {
                const l = p.levels[level].layers[layer];
                if (l.locked || !l.visible) return;
                const rings = terrainContours(d.trail, radius);
                if (!rings.length) return;
                const i = newItem(
                    t("Terrain"),
                    {type: "terrain_region", rings, color: rgba(color)},
                    asset.meta.id,
                );
                i.texture_scale = asset.meta.size.x;
                i.tint = rgba(color);
                const border = asset.meta.id.replace("_tile.", "_border.");
                if (
                    border !== asset.meta.id &&
                    assets.some((a) => a.meta.id === border)
                )
                    i.border = border;
                addTerrainRegion(l, i, radius);
            });
        }
    };
    const updateLayer = (change: (l: Layer) => void) =>
        void edit((p) => change(p.levels[level].layers[layer]));
    const groupTabs = tools.filter(o => o.group === toolGroup).map(o => ({...o, tabId: o.id as string}));
    if (toolGroup === "lighting") groupTabs.push({
        ...tools.find(o => o.id === "light")!,
        tabId: "environment", label: "Environment", key: "", icon: Sun,
    });
    const activeTab = tool === "light" && lightTab === "environment" ? "environment" : tool;
    const chooseToolTab = (tab: typeof groupTabs[number]) => {
        chooseTool(tab.id);
        if (tab.id === "light") setLightTab(tab.tabId === "environment" ? "environment" : "sources");
    };
    const title = groupTabs.find(o => o.tabId === activeTab) ?? tools.find(o => o.id === tool)!;
    const hint =
        tool === "prefab" ? "Prefab placement hint" :
            tool === "path" ? pathEditing ? "Path edit hint" : "Path hint" :
                tool === "roof" ? roofMode === "manual" ? "Roof manual hint" : "Roof quickbox hint" :
                    tool === "light" && lightTab === "environment"
                        ? "Environment color hint"
                        : tool === "select"
                            ? geometryMode === "hole" ? "Hole hint" : geometryMode === "points" ? "Geometry hint" : geometryMode === "resize" ? "Resize hint" : "Select hint"
                            : tool === "room"
                                ? drawShape === "polygon" ? "Draw shape" : drawShape === "circle" ? "Draw circle" : "Draw room"
                                : tool === "floor" || tool === "wall"
                                    ? drawShape === "polygon" ? "Draw shape" : drawShape === "circle" ? "Draw circle" : "Draw room"
                                    : tool === "pan"
                                        ? "Pan hint"
                                        : tool === "terrain" && terrainMode === "water" && waterShape === "polygon"
                                            ? "Draw shape"
                                            : tool === "cave" ? "Cave hint" : tool === "terrain" || tool === "erase"
                                                ? "Paint hint"
                                                : tool === "scatter" ? "Scatter hint" : tool === "text" ? (textMode === "move" ? "Text move hint" : "Text edit hint") : "Place";
    const colorField = (
        label: string,
        value: string,
        onChange: (v: string) => void,
    ) => (
        <ColorField label={t(label)} value={value} onChange={onChange}/>
    );
    const textField = (
        label: string,
        value: string,
        onChange: (v: string) => void,
    ) => (
        <label className="field">
            <span>{t(label)}</span>
            <input
                key={value}
                defaultValue={value}
                onBlur={(e) => {
                    if (e.target.value !== value) onChange(e.target.value);
                }}
                onKeyDown={(e) => {
                    if (e.key === "Enter") e.currentTarget.blur();
                }}
            />
        </label>
    );
    const modalHeader = (title: string, description?: string) => (
        <div className="modal-heading">
            <div className="eyebrow">MAPLOOM STUDIO</div>
            <h2>{t(title)}</h2>
            {description && <p>{t(description)}</p>}
            <IconButton label={t("Close")} onClick={() => setModal(null)}>
                <X size={20}/>
            </IconButton>
        </div>
    );
    const closeAfter = (promise: Promise<unknown>) =>
        void promise
            .then((result) => {
                if (result !== false) setModal(null);
            })
            .catch(() => {
            });

    return (
        <div
            data-theme-part="app"
            data-theme-api="1"
            data-theme-mode={themes.mode}
            className={`studio ${dark ? "dark" : "light"}`}
            style={
                {
                    "--user-accent": `rgb(${data.preferences.accent.join(",")})`,
                } as React.CSSProperties
            }
        >
            {updates.notified && !updates.installing && modal !== "settings" &&
                <aside className="update-notice" role="status">
                    <Download size={18}/>
                    <span>{t("Update available")} {updates.info?.version}</span>
                    <button className="button" onClick={() => {
                        setSettingsTab("updates");
                        setModal("settings");
                    }}>{t("Update open settings")}</button>
                    <button className="icon-button" aria-label={t("Close")} onClick={updates.dismiss}><X size={16}/>
                    </button>
                </aside>}
            {updates.installing &&
                <div className="update-install-overlay" role="dialog" aria-modal="true" aria-label={t("Updates")}>
                    <div className="update-install-card">
                        <LoaderCircle className="spin" size={28}/>
                        <h3>{t(updates.progress?.phase === "installing" ? "Update installing" : "Update downloading")}</h3>
                        <p>{t("Update restart hint")}</p>
                        <progress max={updates.progress?.total || undefined}
                                  value={updates.progress?.total ? updates.progress.downloaded : undefined}/>
                    </div>
                </div>}
            {data.project_open ? <>
                <header className="topbar" data-theme-part="header">
                    <div className="wordmark">
                        <img src={brand} alt=""/>
                        <span>
            maploom<span className="studio-label">STUDIO</span>
          </span>
                    </div>
                    <div className="header-divider"/>
                    <div className="file-menu-wrap">
                        <button
                            className="text-button"
                            onClick={() => setFileMenu((v) => !v)}
                        >
                            {t("File")}
                            <ChevronDown size={13}/>
                        </button>
                        {fileMenu && (
                            <>
                                <button
                                    className="menu-dismiss"
                                    aria-label={t("Close")}
                                    onClick={() => setFileMenu(false)}
                                />
                                <div className="dropdown">
                                    {[
                                        {
                                            label: "New",
                                            icon: FilePlus2,
                                            action: newDialog,
                                            key: "Ctrl N",
                                        },
                                        {
                                            label: "Open",
                                            icon: FolderOpen,
                                            action: () => void run({type: "open", path: null}),
                                            key: "Ctrl O",
                                        },
                                        {
                                            label: "Save",
                                            icon: Save,
                                            action: () =>
                                                void run({type: "save", save_as: false}, "Saved toast"),
                                            key: "Ctrl S",
                                        },
                                        {
                                            label: "Save as",
                                            icon: Copy,
                                            action: () =>
                                                void run({type: "save", save_as: true}, "Saved toast"),
                                            key: "Ctrl Shift S",
                                        },
                                        {
                                            label: "Recover",
                                            icon: History,
                                            action: () => void run({type: "recover"}),
                                            key: "",
                                        },
                                    ].map((o) => (
                                        <button
                                            key={o.label}
                                            onClick={() => {
                                                setFileMenu(false);
                                                o.action();
                                            }}
                                        >
                                            <o.icon size={16}/>
                                            {t(o.label)}
                                            <kbd>{o.key}</kbd>
                                        </button>
                                    ))}
                                </div>
                            </>
                        )}
                    </div>
                    <IconButton
                        label={`${t("Undo")} · Ctrl Z`}
                        onClick={() => void run({type: "undo"})}
                        disabled={!data.undo || !!busy}
                    >
                        <Undo2 size={17}/>
                    </IconButton>
                    <IconButton
                        label={`${t("Redo")} · Ctrl Shift Z`}
                        onClick={() => void run({type: "redo"})}
                        disabled={!data.redo || !!busy}
                    >
                        <Redo2 size={17}/>
                    </IconButton>
                    <button
                        className="command-trigger"
                        onClick={() => {
                            setModal("commands");
                            setCommandQuery("");
                        }}
                        title={t("Commands")}
                    >
                        <Search size={15}/>
                        <span>{t("Commands")}</span>
                        <kbd>Ctrl K</kbd>
                    </button>
                    <IconButton label={t("Report")} onClick={() => setModal("report")}>
                        <History size={17}/>
                        {data.warnings.length > 0 && <span className="notification-dot"/>}
                    </IconButton>
                    <button
                        className="button subtle"
                        onClick={() =>
                            void run({type: "save", save_as: false}, "Saved toast")
                        }
                    >
                        <Save size={15}/>
                        {t("Save")}
                    </button>
                    <button
                        className="button primary"
                        onClick={() => setModal("export")}
                    >
                        <Download size={15}/>
                        {t("Export")}
                        <ChevronDown size={13}/>
                    </button>
                </header>
                <div className="workspace" data-theme-part="workspace">
                    <nav className="tool-rail" data-theme-part="tools" aria-label={t("Tools")}>
                        <div className="rail-tools">
                            {tools.filter(o => o.id === "select" || o.id === "pan" || o.id === "erase").map((o) => (
                                <div
                                    key={o.id}
                                >
                                    <IconButton
                                        label={`${t(o.label)} [${o.key}]`}
                                        active={tool === o.id}
                                        onClick={() => chooseTool(o.id)}
                                    >
                                        <o.icon size={21}/>
                                    </IconButton>
                                </div>
                            ))}
                            <div className="rail-tool-groups tool-separator" role="group"
                                 aria-label={t("Tool categories")}>
                                {toolGroups.map(group => <IconButton key={group.id}
                                                                     label={t(group.label)}
                                                                     active={toolGroup === group.id}
                                                                     onClick={() => chooseTool(lastGroupTools.current[group.id] ?? tools.find(o => o.group === group.id)!.id)}>
                                    <group.icon size={21}/>
                                </IconButton>)}
                            </div>
                            <div className="rail-tool-groups tool-separator">
                                {tools.filter(o => !o.group && o.id !== "select" && o.id !== "pan" && o.id !== "erase").map(o =>
                                    <IconButton
                                        key={o.id}
                                        label={`${t(o.label)} [${o.key}]`} active={tool === o.id}
                                        onClick={() => chooseTool(o.id)}>
                                        <o.icon size={21}/>
                                    </IconButton>)}
                            </div>
                        </div>
                        <div className="rail-bottom">
                            <IconButton label={t("Help")} onClick={() => setModal("help")}>
                                <Keyboard size={20}/>
                            </IconButton>
                            <IconButton
                                label={t("Settings")}
                                onClick={() => setModal("settings")}
                            >
                                <Settings2 size={20}/>
                            </IconButton>
                        </div>
                    </nav>
                    <aside className="structure-panel">
                        <button className="map-settings-trigger" onClick={() => setModal("map-settings")}>
                            <MapIcon size={18}/>
                            <span>{t("Map settings")}<small>{t(activeLayer.name)}</small></span>
                            <Settings2 size={16}/>
                        </button>
                        {toolGroup && <>
                            <div className="section-heading tool-group-heading">
                                <span>{t(toolGroups.find(group => group.id === toolGroup)!.label)}</span>
                            </div>
                            <div className="tool-group-tabs" role="tablist"
                                 aria-label={t(toolGroups.find(group => group.id === toolGroup)!.label)}>
                                {groupTabs.map((o, index, siblings) => <button key={o.tabId}
                                                                               id={`tool-tab-${o.tabId}`}
                                                                               role="tab"
                                                                               aria-selected={activeTab === o.tabId}
                                                                               aria-controls="tool-controls-panel"
                                                                               tabIndex={activeTab === o.tabId ? 0 : -1}
                                                                               className={activeTab === o.tabId ? "active" : ""}
                                                                               title={o.key ? `${t(o.label)} [${o.key}]` : t(o.label)}
                                                                               onClick={() => chooseToolTab(o)}
                                                                               onKeyDown={e => {
                                                                                   const next = e.key === "ArrowRight" ? (index + 1) % siblings.length
                                                                                       : e.key === "ArrowLeft" ? (index + siblings.length - 1) % siblings.length
                                                                                           : e.key === "Home" ? 0 : e.key === "End" ? siblings.length - 1 : null;
                                                                                   if (next === null) return;
                                                                                   e.preventDefault();
                                                                                   chooseToolTab(siblings[next]);
                                                                                   document.getElementById(`tool-tab-${siblings[next].tabId}`)?.focus();
                                                                               }}>
                                    <o.icon size={16}/>
                                    <span>{t(o.label)}</span>
                                </button>)}
                            </div>
                        </>}
                        {hasToolControls && <>
                            <div className="tool-controls" id="tool-controls-panel"
                                 role={toolGroup ? "tabpanel" : undefined}
                                 aria-labelledby={toolGroup ? `tool-tab-${activeTab}` : undefined}>
                                <label className="field">
                                    <span>{t("Layer")}</span>
                                    <select aria-label={t("Layer")} value={layer} onChange={e => {
                                        setLayer(Number(e.target.value));
                                        setSelected([]);
                                        setGesture([]);
                                        updateDrag(null);
                                        setGeometryDraft(null);
                                    }}>
                                        {[...activeLevel.layers].reverse().map((l, n) => (
                                            <option key={l.id} value={activeLevel.layers.length - n - 1}
                                                    disabled={l.locked || !l.visible}>{t(l.name)}</option>
                                        ))}
                                    </select>
                                </label>
                                {tool === "path" && <div className="geometry-controls">
                                    <label className="field"><span>{t("Mode")}</span><select
                                        aria-label={t("Mode")} value={pathMode}
                                        onChange={e => changePathMode(e.target.value as typeof pathMode)}>
                                        <option value="points">{t("Path points")}</option>
                                        <option value="freehand">{t("Freehand")}</option>
                                        <option value="edit">{t("Edit paths")}</option>
                                    </select></label>
                                    {pathEditing ? <>
                                        <p className="muted tiny">{t("Path edit hint")}</p>
                                        {selectedPath && <>
                                            <strong>{selectedPath.name}</strong>
                                            <button disabled={!editableItem}
                                                    onClick={browsePathTexture}>{t("Path texture")}</button>
                                            <button onClick={() => {
                                                setTab("inspector");
                                                setRight(true);
                                            }}>{t("Inspector")}</button>
                                        </>}
                                    </> : <>
                                        <label className="field"><span>{t("Sorting")}</span><select
                                            value={pathUnder ? "under" : "over"}
                                            onChange={e => setPathUnder(e.target.value === "under")}>
                                            <option value="over">{t("Over")}</option>
                                            <option value="under">{t("Under")}</option>
                                        </select></label>
                                        <PathFields t={t} width={pathWidth} style={pathStyle} onChange={(w, s) => {
                                            setPathWidth(w);
                                            setPathStyle(s);
                                        }}/>
                                        <RangeField label={t("Texture scale")} unit="×" min={0.1} max={8} step={0.1}
                                                    value={pathScale} onChange={setPathScale}/>
                                        <p className="muted tiny">{t("Path hint")}</p>
                                        {pathMode === "points" && gesture.length >= 2 &&
                                            <button onClick={finishShape}>{t("Finish shape")}</button>}
                                        {item?.kind.type === "path" &&
                                            <button onClick={() => changePathMode("edit")}>{t("Edit paths")}</button>}
                                    </>}
                                </div>}
                                {tool === "roof" && <div className="geometry-controls">
                                    <label className="field"><span>{t("Mode")}</span><select value={roofMode}
                                                                                             onChange={e => {
                                                                                                 setRoofMode(e.target.value as typeof roofMode);
                                                                                                 setGesture([]);
                                                                                             }}>
                                        <option value="quickbox">{t("Quickbox")}</option>
                                        <option value="manual">{t("Manual")}</option>
                                    </select></label>
                                    <label className="field"><span>{t("Sorting")}</span><select
                                        value={roofUnder ? "under" : "over"}
                                        onChange={e => setRoofUnder(e.target.value === "under")}>
                                        <option value="over">{t("Over")}</option>
                                        <option value="under">{t("Under")}</option>
                                    </select></label>
                                    <RoofFields t={t} showWidth={roofMode === "manual"} value={{
                                        width: roofWidth,
                                        roof_type: roofType,
                                        shade: roofShade,
                                        sun_direction: roofSun,
                                        contrast: roofContrast
                                    }}
                                                onChange={v => {
                                                    setRoofWidth(v.width);
                                                    setRoofType(v.roof_type);
                                                    setRoofShade(v.shade);
                                                    setRoofSun(v.sun_direction);
                                                    setRoofContrast(v.contrast);
                                                }}/>
                                    <RangeField label={t("Texture scale")} unit={t("cells")} min={0.1} max={64}
                                                step={0.25}
                                                value={(assetFor("roofs")?.meta.size.x ?? 4) * floorScale}
                                                onChange={v => setFloorScale(v / (assetFor("roofs")?.meta.size.x ?? 4))}/>
                                    <p className="muted tiny">{t(roofMode === "manual" ? "Roof manual hint" : "Roof quickbox hint")}</p>
                                    {roofMode === "manual" && gesture.length >= 2 &&
                                        <button onClick={finishShape}>{t("Finish shape")}</button>}
                                </div>}
                                {["room", "floor", "wall"].includes(tool) && <div className="geometry-controls">
                                    <span className="muted tiny">{t("Shape")}</span>
                                    <div className="water-shapes" role="group" aria-label={t("Shape")}>
                                        {([["rectangle", Square, "Rectangle"], ["circle", Circle, "Circle"], ["polygon", Pentagon, "Polygon"]] as const).map(([shape, Icon, label]) =>
                                            <IconButton key={shape} label={t(label)} active={drawShape === shape}
                                                        onClick={() => {
                                                            setDrawShapes(s => ({...s, [tool]: shape}));
                                                            setGesture([]);
                                                        }}><Icon size={19}/></IconButton>)}
                                    </div>
                                    {drawShape === "circle" && <>
                                        <RangeField label={t("Segmentation")} unit="" value={segmentation} min={6}
                                                    max={64}
                                                    step={1} onChange={setSegmentation}/>
                                        <p className="muted tiny">{segmentation === 64 ? t("Smooth circle") : `${segmentation} ${t("sides")}`} · {t("Circle segmentation hint")}</p>
                                    </>}
                                    {tool === "room" &&
                                        <Toggle label={t("Room walls")} checked={newRoomWalls}
                                                onChange={setNewRoomWalls}/>}
                                </div>}
                                {tool === "terrain" &&
                                    <div className="terrain-modes" role="group" aria-label={t("Terrain mode")}>
                                        <button className={terrainMode === "terrain" ? "active" : ""}
                                                aria-pressed={terrainMode === "terrain"}
                                                onClick={() => {
                                                    setTerrainMode("terrain");
                                                    setGesture([]);
                                                }}><Mountain size={16}/>{t("Terrain")}</button>
                                        <button className={terrainMode === "water" ? "active" : ""}
                                                aria-pressed={terrainMode === "water"}
                                                onClick={() => {
                                                    setTerrainMode("water");
                                                    setGesture([]);
                                                }}><Droplets size={16}/>{t("Water")}</button>
                                    </div>}
                                {tool === "light" ? <>
                                    {lightTab === "environment" ?
                                        <div className="environment-fields"
                                             aria-label={t("Environment")}>
                                            <ColorField label={t("Color")}
                                                        value={hex(data.project.ambient_light ?? white)}
                                                        onChange={color => void edit(p => {
                                                            p.ambient_light = rgba(color);
                                                        })}/>
                                            <p className="muted tiny">{t("Environment color hint")}</p>
                                        </div> :
                                        <fieldset className="item-fields"
                                                  aria-label={t("Light sources")}
                                                  disabled={item?.kind.type === "light" && !editableItem}>
                                            <LightFields value={item?.kind.type === "light" ? item.kind : light} t={t}
                                                         onChange={value => {
                                                             setLight(value);
                                                             if (item?.kind.type === "light") changeItem(i => {
                                                                 if (i.kind.type === "light") Object.assign(i.kind, value);
                                                             });
                                                         }}/>
                                        </fieldset>}
                                </> : tool === "cave" ? <>
                                    <div className="terrain-modes" role="group" aria-label={t("Brush mode")}>
                                        <button className={caveMode === "dig" ? "active" : ""}
                                                aria-pressed={caveMode === "dig"}
                                                onClick={() => setCaveMode("dig")}>{t("Dig Cave")}</button>
                                        <button className={caveMode === "blast" ? "active" : ""}
                                                aria-pressed={caveMode === "blast"}
                                                onClick={() => setCaveMode("blast")}>{t("Blast Open")}</button>
                                    </div>
                                    <Toggle label={t("Show blast opened area")} checked={showBlasts}
                                            onChange={setShowBlasts}/>
                                    <div className="water-brushes" role="group" aria-label={t("Brush size")}>
                                        {[0.5, 1, 2, 4, 8].map((r, n) => <button key={r}
                                                                                 aria-label={`${t("Radius")} ${r}`}
                                                                                 aria-pressed={radius === r}
                                                                                 className={radius === r ? "active" : ""}
                                                                                 onClick={() => setRadius(r)}>
                                            <span style={{width: 6 + n * 4, height: 6 + n * 4}}/></button>)}
                                    </div>
                                    <CaveFields value={cave} onChange={setCave} styles={caveMaterials}
                                                material={activeCaveMaterial?.id ?? ""}
                                                onMaterial={s => setCaveMaterial(s.id)} t={t}/>
                                    <p className="muted tiny">{t("Cave hint")}</p>
                                </> : tool === "terrain" && terrainMode === "water" ? <>
                                    <label className="field"><span>{t("Shape")}</span></label>
                                    <div className="water-shapes" role="group" aria-label={t("Shape")}>
                                        {([["circle", Circle, "Circle"], ["square", Square, "Square"], ["polygon", Pentagon, "Polygon"]] as const).map(([shape, Icon, label]) =>
                                            <IconButton key={shape} label={t(label)} active={waterShape === shape}
                                                        onClick={() => {
                                                            setWaterShape(shape);
                                                            setGesture([]);
                                                        }}><Icon size={19}/></IconButton>)}
                                    </div>
                                    {waterShape !== "polygon" &&
                                        <div className="water-brushes" role="group" aria-label={t("Brush size")}>
                                            {[0.5, 1, 2, 4, 8].map((r, index) => <button key={r}
                                                                                         aria-label={`${t("Radius")} ${r}`}
                                                                                         aria-pressed={radius === r}
                                                                                         className={radius === r ? "active" : ""}
                                                                                         onClick={() => setRadius(r)}>
                                                <span style={{width: 6 + index * 4, height: 6 + index * 4}}/>
                                            </button>)}
                                        </div>}
                                    <WaterFields value={water} onChange={setWater} t={t}/>
                                    <p className="muted tiny">{t(waterShape === "polygon" ? "Water polygon hint" : "Water brush hint")}</p>
                                </> : tool === "room" || selectedRoom ? (
                                    <>
                                        <fieldset
                                            disabled={!!selectedRoom && !editableItem}
                                            className="item-fields"
                                        >
                                            {colorField(
                                                "Floor color",
                                                selectedRoom
                                                    ? hex(roomTint(selectedRoom, true))
                                                    : floorColor,
                                                (c) => {
                                                    setFloorColor(c);
                                                    if (selectedRoom)
                                                        changeItem((i) => setRoomTint(i, true, rgba(c)));
                                                },
                                            )}
                                            {colorField(
                                                "Wall color",
                                                selectedRoom
                                                    ? hex(roomTint(selectedRoom, false))
                                                    : wallColor,
                                                (c) => {
                                                    setWallColor(c);
                                                    if (selectedRoom)
                                                        changeItem((i) => setRoomTint(i, false, rgba(c)));
                                                },
                                            )}
                                        </fieldset>
                                    </>
                                ) : tool === "text" ? null : (
                                    colorField("Color", color, (v) => {
                                        setColor(v);
                                        if (item && editableItem)
                                            changeItem((i) => {
                                                if (i.kind.type === "room") {
                                                    i.floor_tint = rgba(v);
                                                    i.wall_tint = rgba(v);
                                                } else {
                                                    i.tint = rgba(v);
                                                    if ("color" in i.kind) i.kind.color = rgba(v);
                                                }
                                            });
                                    })
                                )}
                                {["wall", "room"].includes(tool) && (
                                    <NumberField
                                        label={t("Width")}
                                        value={width}
                                        min={0.01}
                                        max={5}
                                        step={0.01}
                                        onChange={setWidth}
                                    />
                                )}
                                {["room", "floor"].includes(tool) && (
                                    <RangeField
                                        label={t("Floor texture scale")}
                                        unit={t("cells")}
                                        value={(assetFor("tilesets")?.meta.size.x ?? 1) * floorScale}
                                        onChange={(value) => setFloorScale(value / (assetFor("tilesets")?.meta.size.x ?? 1))}
                                    />
                                )}
                                {(tool === "cave" || tool === "terrain" && !(terrainMode === "water" && waterShape === "polygon")) && (
                                    <NumberField
                                        label={t("Radius")}
                                        value={radius}
                                        min={0.1}
                                        max={32}
                                        step={0.1}
                                        onChange={setRadius}
                                    />
                                )}
                                {tool === "object" && (
                                    <>
                                        <Toggle label={t("Block light")} checked={objectBlocksLight}
                                                onChange={setObjectBlocksLight}/>
                                        <Toggle label={t("Snap to walls")} checked={wallSnap} onChange={setWallSnap}/>
                                        <p className="muted tiny">{t("Wall snap hint")}</p>
                                        <NumberField
                                            label={t("Scale")}
                                            value={objectScale}
                                            min={0.05}
                                            max={100}
                                            step={0.05}
                                            onChange={setObjectScale}
                                        />
                                        <NumberField
                                            label={t("Rotation")}
                                            value={rotation}
                                            min={-360}
                                            max={360}
                                            step={15}
                                            onChange={setRotation}
                                        />
                                        <p className="muted tiny">{t("Object wheel hint")}</p>
                                    </>
                                )}
                                {tool === "prefab" && <>
                                    <p className="muted tiny">{t("Prefab placement hint")}</p>
                                    <NumberField label={t("Scale")} value={objectScale} min={0.05} max={100} step={0.05}
                                                 onChange={setObjectScale}/>
                                    <NumberField label={t("Rotation")} value={rotation} min={-360} max={360} step={15}
                                                 onChange={setRotation}/>
                                </>}
                                {tool === "scatter" && <>
                                    <p className="muted tiny">{t("Scatter selection hint")}</p>
                                    <div className="scatter-selection">
                                        <span>{t("Selected assets")}: <strong>{scatterAssets.length}</strong></span>
                                        <button className="button subtle compact" disabled={!scatterIds.length}
                                                onClick={() => setScatterIds([])}>{t("Clear selection")}</button>
                                    </div>
                                    <DualRangeField label={t("Rotation")} unit="°" value={scatterSettings.rotation}
                                                    min={-180} max={180} step={1} minimumLabel={t("Minimum")}
                                                    maximumLabel={t("Maximum")}
                                                    onChange={rotation => setScatterSettings(s => ({...s, rotation}))}/>
                                    <DualRangeField label={t("Scale")} unit="×" value={scatterSettings.scale}
                                                    min={0.05} max={5} step={0.05} minimumLabel={t("Minimum")}
                                                    maximumLabel={t("Maximum")}
                                                    onChange={scale => setScatterSettings(s => ({...s, scale}))}/>
                                    <RangeField label={t("Spread")} unit={t("cells")} value={scatterSettings.spread}
                                                min={0.1} max={5} step={0.1}
                                                onInput={spread => setScatterSettings(s => ({...s, spread}))}
                                                onChange={spread => setScatterSettings(s => ({...s, spread}))}/>
                                    <p className="muted tiny">{t("Scatter spread hint")}</p>
                                    <Toggle label={t("Random colors")} checked={scatterSettings.randomColors}
                                            onChange={randomColors => setScatterSettings(s => ({...s, randomColors}))}/>
                                    {scatterSettings.randomColors && <>
                                        <div className="scatter-palette" role="group" aria-label={t("Scatter palette")}>
                                            {scatterSettings.colors.map((entry, index) => <button key={index}
                                                                                                  className="scatter-swatch"
                                                                                                  style={{background: hex(entry)}}
                                                                                                  aria-label={`${t("Remove color")} ${hex(entry)}`}
                                                                                                  title={`${t("Remove color")} ${hex(entry)}`}
                                                                                                  onClick={() => setScatterSettings(s => ({
                                                                                                      ...s,
                                                                                                      colors: s.colors.filter((_, n) => n !== index)
                                                                                                  }))}/>)}
                                            <AddColorButton label={t("Add color")} value={color}
                                                            onAdd={chosen => setScatterSettings(s => s.colors.some(c => hex(c) === chosen)
                                                                ? s : {...s, colors: [...s.colors, rgba(chosen)]})}/>
                                        </div>
                                        <p className="muted tiny">{t("Scatter palette hint")}</p>
                                    </>}
                                </>}
                                {tool === "text" && (
                                    <fieldset className="text-tool-fields"
                                              disabled={item?.kind.type === "text" && !editableItem}>
                                        <TextEditor key={item?.kind.type === "text" ? item.id : "new"}
                                                    value={item?.kind.type === "text" ? item.kind : textDraft}
                                                    style={item?.kind.type === "text" ? item.text_style : textStyle}
                                                    ppg={project.pixels_per_grid} mode={textMode} onMode={setTextMode}
                                                    t={t}
                                                    live={item?.kind.type !== "text"}
                                                    onChange={patch => item?.kind.type === "text" ? changeItem(i => {
                                                        if (i.kind.type === "text") i.kind = {...i.kind, ...patch};
                                                        i.tint = white;
                                                    }) : setTextDraft(v => ({...v, ...patch}))}
                                                    onStyle={style => item?.kind.type === "text" ? changeItem(i => {
                                                        i.text_style = style;
                                                        i.tint = white;
                                                    }) : setTextStyle(style)}
                                                    onNew={() => {
                                                        if (item?.kind.type === "text") {
                                                            setTextDraft({...item.kind, text: ""});
                                                            setTextStyle({...defaultTextStyle(), ...item.text_style});
                                                        }
                                                        setSelected([]);
                                                        setTextMode("edit");
                                                    }}/>
                                    </fieldset>
                                )}
                                {toolCategory[tool] && tool !== "scatter" && !(tool === "terrain" && terrainMode === "water") && (
                                    <div className="material-summary">
                                        {activeAsset ? (
                                            <>
                                                <Thumbnail id={activeAsset.meta.id}
                                                           color={thumbnailColor(activeAsset)}/>
                                                <div>
                                                    <small>{t("Selection")}</small>
                                                    <strong>
                                                        {activeAsset.category === "roofs" ? roofStyleName(activeAsset) : activeAsset.meta.name.replaceAll("_", " ")}
                                                    </strong>
                                                </div>
                                            </>
                                        ) : (
                                            <span className="muted tiny">{t("Choose asset")}</span>
                                        )}
                                    </div>
                                )}
                            </div>
                        </>}
                    </aside>
                    <main className="canvas-panel" data-theme-part="canvas">
                        <div className="document-tabs">
                            <div className="document-tab">
                                <MapIcon size={15}/>
                                <span>{project.name}</span>
                                {data.dirty && <span className="save-dot unsaved"/>}
                            </div>
                            <button className="new-tab" title={t("New")} onClick={newDialog}>
                                <Plus size={16}/>
                            </button>
                            <div className="tabs-end">
                                <small>{t(data.dirty ? "Unsaved" : "Saved")}</small>
                                <IconButton
                                    label={t(right ? "Close" : "Assets")}
                                    onClick={() => setRight((v) => !v)}
                                >
                                    {right ? (
                                        <PanelRightClose size={17}/>
                                    ) : (
                                        <PanelRightOpen size={17}/>
                                    )}
                                </IconButton>
                            </div>
                        </div>
                        <div className="canvas-toolbar">
                            <div className="current-tool">
                                <title.icon size={16}/>
                                <span>{t(title.label)}</span>
                                <kbd>{title.key}</kbd>
                            </div>
                            <div className="toolbar-divider"/>
                            <IconButton
                                label={t("Grid")}
                                active={grid}
                                onClick={() => setGrid((v) => !v)}
                            >
                                <Grid2X2 size={16}/>
                            </IconButton>
                            <IconButton
                                label={t("Snap")}
                                active={snap}
                                onClick={() => setSnap((v) => !v)}
                            >
                                <Magnet size={16}/>
                            </IconButton>
                            <IconButton label={t("Snap to walls")} active={wallSnap}
                                        onClick={() => setWallSnap(v => !v)}>
                                <BrickWall size={16}/>
                            </IconButton>
                            <IconButton
                                label={t("Lights")}
                                active={lights}
                                onClick={() => setLights((v) => !v)}
                            >
                                <Sun size={16}/>
                            </IconButton>
                            <span className="toolbar-spacer"/>
                            <span className="canvas-level">{t(activeLevel.name)}</span>
                            <div className="zoom-control">
                                <IconButton
                                    label="−"
                                    onClick={() => setScale((s) => Math.max(2, s / 1.2))}
                                >
                                    <Minus size={14}/>
                                </IconButton>
                                <button onClick={fit} title={t("Fit")}>
                                    {Math.round((scale / 32) * 100)}%
                                </button>
                                <IconButton
                                    label="+"
                                    onClick={() => setScale((s) => Math.min(400, s * 1.2))}
                                >
                                    <Plus size={14}/>
                                </IconButton>
                            </div>
                            <IconButton label={`${t("Fit")} [F]`} onClick={fit}>
                                <Maximize size={16}/>
                            </IconButton>
                        </div>
                        <div
                            className={`canvas-stage tool-${tool}`}
                            ref={stage}
                            onPointerDown={pointerDown}
                            onPointerEnter={trackPlacement}
                            onPointerMove={pointerMove}
                            onPointerLeave={() => {
                                setGeometryHover(null);
                                setBrushCursor(null);
                                setPlacementCursor(null);
                            }}
                            onPointerUp={pointerUp}
                            onPointerCancel={() => {
                                updateDrag(null);
                                setGesture([]);
                                setGeometryDraft(null);
                                setPlacementCursor(null);
                            }}
                            onDoubleClick={() => {
                                if (tool === "path" && pathMode === "points" || geometryMode === "hole" || (["room", "floor", "wall", "roof"].includes(tool) && drawShape === "polygon") || (tool === "terrain" && terrainMode === "water" && waterShape === "polygon")) finishShape();
                            }}
                            onContextMenu={(e) => e.preventDefault()}
                        >
                            <div className="canvas-coordinate top-left">0 : 0</div>
                            <div className="canvas-coordinate bottom-right">
                                {project.width} : {project.height}
                            </div>
                            <GpuCanvas
                                picker={picker}
                                scene={transientScene ?? gpuScene}
                                retainedScene={transientScene ? gpuScene : null}
                                view={{origin, scale, width: viewport.w, height: viewport.h}}
                                overlay={{
                                    items: geometryDraft ? selection.map(i => i.id === geometryDraft.id ? geometryDraft : i) : selection,
                                    geometryMode: (tool === "select" || pathEditing) && editableItem ? activeGeometryMode : null,
                                    geometryHover: !drag ? geometryHover : null,
                                    vertex,
                                    drawingContour: drag && (drag.mode === "room" || drag.mode === "hole") ? ((drag.mode === "hole" ? holeShape : drawShape) === "circle" ? circleContour(drag.start, drag.last, segmentation) : rectangle(drag.start, drag.last)) : undefined,
                                    roof: tool === "roof" ? roofMode === "quickbox" && drag?.mode === "room"
                                        ? {...roofQuickbox(drag.start, drag.last), roof_type: roofType}
                                        : roofMode === "manual" && gesture.length
                                            ? {
                                                points: [...gesture, ...(roofCursor && !samePoint(roofCursor, gesture.at(-1)!) ? [roofCursor] : [])],
                                                width: roofWidth,
                                                roof_type: roofType
                                            }
                                            : undefined : undefined,
                                    path: tool === "path" && !pathEditing ? {
                                        points: drag?.mode === "path" ? drag.trail : [...gesture, ...(roofCursor && gesture.length && !samePoint(roofCursor, gesture.at(-1)!) ? [roofCursor] : [])],
                                        width: pathWidth,
                                        style: pathStyle
                                    } : undefined,
                                    assets: assetGeometry,
                                    gesture: drawingTool && !drag ? tentativePoints : gesture,
                                    drag,
                                    radius,
                                    brushShape: tool === "terrain" && terrainMode === "water" ? waterShape : "circle",
                                    brushCursor: tool === "cave" || tool === "terrain" && terrainMode === "water" && waterShape !== "polygon" ? brushCursor : null,
                                    caveBlasts,
                                    accent: data.preferences.accent,
                                }}
                                onReady={(revision) => {
                                    if (revision === current.current?.revision) setRendering(false);
                                }}
                                onError={(message) => {
                                    setSceneError(t(message));
                                    setRendering(false);
                                }}
                                onRecovery={() => setSceneEpoch((n) => n + 1)}
                            />
                            <div
                                className="map-surface"
                                style={{
                                    left: origin.x,
                                    top: origin.y,
                                    width: project.width * scale,
                                    height: project.height * scale,
                                }}
                            />
                            {tool === "prefab" && activePrefab && showPlacement && placementPoint &&
                                <PrefabPlacement prefab={activePrefab.prefab} assets={assetGeometry}
                                                 position={placementPoint}
                                                 scale={objectScale} rotation={rotation}
                                                 view={{origin, scale, width: viewport.w, height: viewport.h}}/>}
                            {placementAsset && <ObjectPlacement
                                asset={placementAsset.meta}
                                position={showPlacement ? placementPoint : null}
                                scale={objectScale}
                                rotation={rotation}
                                color={rgba(color)}
                                view={{origin, scale, width: viewport.w, height: viewport.h}}
                            />}
                            {tool === "scatter" && nextScatter?.kind.type === "object" && !drag && <ObjectPlacement
                                asset={assetGeometry[nextScatter.kind.asset]}
                                position={showPlacement ? placementPoint : null}
                                scale={nextScatter.kind.size.x / assetGeometry[nextScatter.kind.asset].size.x}
                                rotation={nextScatter.kind.rotation * 180 / Math.PI} color={nextScatter.tint}
                                view={{origin, scale, width: viewport.w, height: viewport.h}}/>}
                            {drag?.mode === "scatter" && drag.items?.map(i => i.kind.type === "object" &&
                                <ObjectPlacement
                                    key={i.id} asset={assetGeometry[i.kind.asset]} position={i.kind.position}
                                    scale={i.kind.size.x / assetGeometry[i.kind.asset].size.x}
                                    rotation={i.kind.rotation * 180 / Math.PI} color={i.tint}
                                    view={{origin, scale, width: viewport.w, height: viewport.h}}/>)}
                            {tool === "portal" && <PortalPlacement request={portalRequest}
                                                                   view={{
                                                                       origin,
                                                                       scale,
                                                                       width: viewport.w,
                                                                       height: viewport.h
                                                                   }}
                                                                   onScene={setPortalScene} t={t}/>}
                            <DrawingPreview request={drawingRequest} scope={drawingScope} onScene={setDrawingScene}/>
                            {sceneError && (
                                <div className="canvas-alert">
                                    <AlertCircle size={16}/>
                                    <span>{sceneError}</span>
                                    <button onClick={() => setModal("report")}>
                                        {t("Report")}
                                    </button>
                                </div>
                            )}
                            <div className="canvas-hint">
                                <kbd>{title.key}</kbd>
                                {t(hint)}
                            </div>
                            <button
                                className="minimap"
                                title={t("Fit")}
                                onPointerDown={(e) => e.stopPropagation()}
                                onClick={fit}
                            >
                                <GpuCanvas
                                    scene={gpuScene}
                                    className="minimap-render"
                                    view={{
                                        origin: {
                                            x:
                                                (104 -
                                                    project.width *
                                                    Math.min(104 / project.width, 62 / project.height)) /
                                                2,
                                            y:
                                                (62 -
                                                    project.height *
                                                    Math.min(104 / project.width, 62 / project.height)) /
                                                2,
                                        },
                                        scale: Math.min(104 / project.width, 62 / project.height),
                                        width: 104,
                                        height: 62,
                                    }}
                                />
                                <span>
                <Compass size={13}/> {t("Map")}
              </span>
                            </button>
                            {rendering && (
                                <div className="scene-loading">
                                    <LoaderCircle size={13} className="spin"/>
                                    {t("Processing")}
                                </div>
                            )}
                        </div>
                    </main>
                    {right && (
                        <aside className="library-panel" data-theme-part="library">
                            <div className="panel-tabs">
                                <button
                                    className={tab === "library" ? "active" : ""}
                                    onClick={() => setTab("library")}
                                >
                                    <Package size={16}/>
                                    {t("Assets")}
                                </button>
                                <button
                                    className={tab === "inspector" ? "active" : ""}
                                    onClick={() => setTab("inspector")}
                                >
                                    <SlidersHorizontal size={16}/>
                                    {t("Inspector")}
                                </button>
                            </div>
                            {tab === "library" ? tool === "prefab" ? (
                                <PrefabsPanel entries={prefabEntries} active={activePrefab?.key} assets={assetGeometry}
                                              canCreate={canSavePrefab} busy={!!busy} t={t} onChoose={setPrefabKey}
                                              onCreate={openPrefabDialog}
                                              onImport={() => void run({type: "import_prefabs"}, "Prefabs imported")}
                                              onExport={keys => void run({type: "export_prefabs", keys}, "Exported")}
                                              onDelete={id => void run({type: "delete_prefab", id}, "Prefab deleted")}/>
                            ) : (
                                <>
                                    <div className="library-controls">
                                        <div className="library-heading">
                                            <h2>{t("Assets")}</h2>
                                            <button
                                                className="button subtle compact"
                                                onClick={() =>
                                                    setModal("catalogs")
                                                }
                                            >
                                                <Plus size={14}/>
                                                {t("Asset catalogs")}
                                            </button>
                                        </div>
                                        <label className="search-box">
                                            <Search size={15}/>
                                            <input
                                                aria-label={t("Search assets")}
                                                placeholder={t("Search assets")}
                                                value={query}
                                                onChange={(e) => setQuery(e.target.value)}
                                            />
                                            {query && (
                                                <button
                                                    onClick={() => setQuery("")}
                                                    aria-label={t("Close")}
                                                >
                                                    <X size={13}/>
                                                </button>
                                            )}
                                        </label>
                                        {(tool === "room" || selectedRoom) && (
                                            <div className="room-material-tabs">
                                                <button
                                                    className={materialTab === "tilesets" ? "active" : ""}
                                                    onClick={() => {
                                                        setMaterialTab("tilesets");
                                                        setCategory("tilesets");
                                                    }}
                                                >
                                                    {t("Floor material")}
                                                </button>
                                                <button
                                                    className={materialTab === "walls" ? "active" : ""}
                                                    onClick={() => {
                                                        setMaterialTab("walls");
                                                        setCategory("walls");
                                                    }}
                                                >
                                                    {t("Wall material")}
                                                </button>
                                            </div>
                                        )}
                                        {selectedRoom && ["tilesets", "walls"].includes(category) && (
                                            <div className="room-material-target">
                      <span>
                        {t("Selection")}: <strong>{selectedRoom.name}</strong>
                      </span>
                                                <button
                                                    onClick={() => setTab("inspector")}
                                                    title={t("Inspector")}
                                                    aria-label={t("Inspector")}
                                                >
                                                    <SlidersHorizontal size={15}/>
                                                </button>
                                            </div>
                                        )}
                                        {selectedPortal && category === "portals" && (
                                            <div className="room-material-target">
                                                <span>{t("Selection")}: <strong>{selectedPortal.name}</strong></span>
                                                <IconButton label={t("Inspector")} onClick={() => setTab("inspector")}>
                                                    <SlidersHorizontal size={15}/>
                                                </IconButton>
                                            </div>
                                        )}
                                        {selectedPath && category === "paths" && (
                                            <div className="room-material-target">
                                                <span>{t("Selection")}: <strong>{selectedPath.name}</strong></span>
                                                <IconButton label={t("Inspector")} onClick={() => setTab("inspector")}>
                                                    <SlidersHorizontal size={15}/>
                                                </IconButton>
                                            </div>
                                        )}
                                        <div className="library-filters">
                                            <select
                                                aria-label={t("Assets")}
                                                value={category}
                                                onChange={(e) => setCategory(e.target.value)}
                                            >
                                                {Object.entries(categories).map(([c, label]) => (
                                                    <option key={c} value={c}>
                                                        {t(label)}
                                                    </option>
                                                ))}
                                            </select>
                                            <IconButton
                                                label={t("Favorites")}
                                                active={favoriteOnly}
                                                onClick={() => setFavoriteOnly((v) => !v)}
                                            >
                                                <Star size={15}/>
                                            </IconButton>
                                            <IconButton
                                                label={t("Fabric")}
                                                onClick={() => setModal("fabric")}
                                            >
                                                <Package size={15}/>
                                            </IconButton>
                                        </div>
                                        <div className="asset-count">
                    <span>
                      {filtered.length.toLocaleString()} {t("items")}
                    </span>
                                            <span>
                      {project.asset_catalogs.length} {t("catalogs")}
                    </span>
                                        </div>
                                    </div>
                                    <div className={`asset-gallery ${category === "paths" ? "path-gallery" : ""}`}>
                                        {(tool === "room" || selectedRoom) &&
                                            category === "tilesets" && (
                                                <button
                                                    className={`asset-card no-floor ${(selectedRoom ? roomPart === null : materials.tilesets === null) ? "active" : ""}`}
                                                    disabled={!!selectedRoom && !editableItem}
                                                    onClick={() => chooseMaterial(null)}
                                                >
                        <span>
                          <X size={22}/>
                        </span>
                                                    <strong>{t("No floor")}</strong>
                                                </button>
                                            )}
                                        {filtered
                                            .slice(page * pageSize, (page + 1) * pageSize)
                                            .map((a) => (
                                                <div
                                                    className={`asset-card ${(tool === "scatter" && a.category === "objects" ? scatterIds.includes(a.meta.id) : activeAsset?.meta.id === a.meta.id) ? "active" : ""}`}
                                                    key={a.meta.id}
                                                >
                                                    <button
                                                        className="asset-select"
                                                        title={`${a.meta.name}\n${a.meta.tags.join(", ")}`}
                                                        disabled={
                                                            ((!!selectedRoom && ["tilesets", "walls"].includes(category)) ||
                                                                (!!selectedPortal && category === "portals") ||
                                                                (!!selectedPath && category === "paths")) &&
                                                            !editableItem
                                                        }
                                                        aria-pressed={tool === "scatter" && a.category === "objects" ? scatterIds.includes(a.meta.id) : activeAsset?.meta.id === a.meta.id}
                                                        onClick={e => {
                                                            if (a.category === "objects" && (tool === "scatter" || (tool === "object" && e.shiftKey))) {
                                                                setScatterIds(ids => chooseScatterAssets(tool === "object" && !ids.length && activeAsset ? [activeAsset.meta.id] : ids, a.meta.id, e.shiftKey));
                                                                if (tool !== "scatter") chooseTool("scatter");
                                                            } else chooseMaterial(a);
                                                        }}
                                                    >
                                                        <Thumbnail id={a.meta.id} color={thumbnailColor(a)}
                                                                   frameClassName="asset-image"/>
                                                        <strong>{a.category === "roofs" ? roofStyleName(a) : a.meta.name.replaceAll("_", " ")}</strong>
                                                    </button>
                                                    <button
                                                        className={`favorite-button ${favorites.includes(a.meta.id) ? "starred" : ""}`}
                                                        aria-label={t("Favorites")}
                                                        title={t("Favorites")}
                                                        onClick={() =>
                                                            setFavorites((f) =>
                                                                f.includes(a.meta.id)
                                                                    ? f.filter((id) => id !== a.meta.id)
                                                                    : [...f, a.meta.id],
                                                            )
                                                        }
                                                    >
                                                        <Star
                                                            size={12}
                                                            fill={
                                                                favorites.includes(a.meta.id)
                                                                    ? "currentColor"
                                                                    : "none"
                                                            }
                                                        />
                                                    </button>
                                                </div>
                                            ))}
                                        {filtered.length === 0 && (
                                            <div className="library-empty">
                                                <ImagePlus size={30}/>
                                                <strong>{t("No assets")}</strong>
                                                <p>{t("Import hint")}</p>
                                                <button
                                                    className="button subtle"
                                                    onClick={() =>
                                                        setModal("catalogs")
                                                    }
                                                >
                                                    {t("Asset catalogs")}
                                                </button>
                                            </div>
                                        )}
                                    </div>
                                    <div className="gallery-pagination">
                                        <IconButton
                                            label={t("Previous")}
                                            disabled={page === 0}
                                            onClick={() => setPage((p) => p - 1)}
                                        >
                                            <ChevronLeft size={15}/>
                                        </IconButton>
                                        <span>
                    {t("Page")} {page + 1} / {pages}
                  </span>
                                        <IconButton
                                            label={t("Next")}
                                            disabled={page >= pages - 1}
                                            onClick={() => setPage((p) => p + 1)}
                                        >
                                            <ChevronRight size={15}/>
                                        </IconButton>
                                    </div>
                                    {activeAsset && (
                                        <div className="asset-detail">
                                            <div className="asset-detail-heading">
                                                <Thumbnail id={activeAsset.meta.id}
                                                           color={thumbnailColor(activeAsset)}/>
                                                <div>
                                                    <strong>
                                                        {activeAsset.category === "roofs" ? roofStyleName(activeAsset) : activeAsset.meta.name.replaceAll("_", " ")}
                                                    </strong>
                                                    <small>
                                                        {activeAsset.meta.size.x.toFixed(2)} ×{" "}
                                                        {activeAsset.meta.size.y.toFixed(2)} {t("cells")}
                                                    </small>
                                                </div>
                                                <IconButton
                                                    label={t("Edit asset")}
                                                    onClick={() => {
                                                        setVariantName(activeAsset.meta.name + " variant");
                                                        setModal("asset");
                                                    }}
                                                >
                                                    <Scissors size={15}/>
                                                </IconButton>
                                            </div>
                                            <div className="tag-list">
                                                {activeAsset.meta.tags.slice(0, 3).map((tag) => (
                                                    <span key={tag}>{tag}</span>
                                                ))}
                                            </div>
                                        </div>
                                    )}
                                </>
                            ) : (
                                <div className="inspector-content">
                                    <div className="inspector-title">
                                        <span className="eyebrow">{t("Selection")}</span>
                                        <h2>
                                            {item
                                                ? item.name
                                                : selection.length
                                                    ? `${selection.length} ${t("items")}`
                                                    : t("Inspector")}
                                        </h2>
                                    </div>
                                    {selection.length === 0 ? (
                                        <div className="selection-empty">
                                            <MousePointer2 size={28}/>
                                            <h3>{t("Nothing selected")}</h3>
                                            <p>{t("Selection hint")}</p>
                                            <kbd>V</kbd>
                                        </div>
                                    ) : (
                                        <>
                                            <div className="selection-actions">
                                                <button
                                                    className="button subtle"
                                                    onClick={duplicateSelection}
                                                >
                                                    <Copy size={15}/>
                                                    {t("Duplicate")}
                                                </button>
                                                <IconButton label={t("Delete")} onClick={deleteSelection}>
                                                    <Trash2 size={16}/>
                                                </IconButton>
                                            </div>
                                            {canSavePrefab &&
                                                <button className="button subtle create-prefab-button" disabled={!!busy}
                                                        onClick={openPrefabDialog}>
                                                    <Package size={16}/>{t("Create prefab")}
                                                </button>}
                                            <fieldset className="item-fields"
                                                      disabled={!selection.some(i => !i.locked && !activeLevel.layers.find(l => l.items.includes(i))?.locked)}>
                                                <label className="field">
                                                    <span>{t("Move to layer")}</span>
                                                    <select
                                                        aria-label={t("Move to layer")}
                                                        value={activeLevel.layers.find(l => selection.length > 0 && selection.every(i => l.items.includes(i)))?.id ?? ""}
                                                        onChange={(e) => {
                                                            const targetId = e.target.value;
                                                            void edit(p => moveToLayer(p.levels[level], selected, targetId)).then(ok => {
                                                                if (ok) setLayer(activeLevel.layers.findIndex(l => l.id === targetId));
                                                            });
                                                        }}
                                                    >
                                                        <option value="" disabled>{t("Multiple layers")}</option>
                                                        {[...activeLevel.layers].reverse().map(l => <option key={l.id}
                                                                                                            value={l.id}
                                                                                                            disabled={l.locked || !l.visible}>{t(l.name)}</option>)}
                                                    </select>
                                                </label>
                                                <span className="muted tiny">{t("Order in layer")}</span>
                                                <div className="stack-actions">
                                                    {([
                                                        ["back", "Send to back", ChevronsDown],
                                                        ["down", "Lower", ArrowDown],
                                                        ["up", "Raise", ArrowUp],
                                                        ["front", "Bring to front", ChevronsUp],
                                                    ] as const).map(([order, label, Icon]) => (
                                                        <IconButton key={order} label={t(label)}
                                                                    onClick={() => void edit(p => reorderItems(p.levels[level], selected, order))}>
                                                            <Icon size={17}/>
                                                        </IconButton>
                                                    ))}
                                                </div>
                                            </fieldset>
                                            {item && (
                                                <>
                                                    <fieldset
                                                        disabled={!editableItem}
                                                        className="item-fields"
                                                    >
                                                        {textField("Name", item.name, (name) =>
                                                            changeItem((i) => {
                                                                i.name = name;
                                                            }),
                                                        )}
                                                        {item.kind.type === "path" &&
                                                            <div className="geometry-controls">
                                                                <PathFields t={t} width={item.kind.width}
                                                                            style={{...defaultPathStyle(), ...item.path_style}}
                                                                            onChange={(w, s) => changeItem(i => {
                                                                                if (i.kind.type !== "path") return;
                                                                                i.texture_scale *= w / i.kind.width;
                                                                                i.kind.width = w;
                                                                                i.path_style = s;
                                                                            })}/>
                                                                <RangeField label={t("Texture scale")} unit={t("cells")}
                                                                            min={0.01} max={128} step={0.05}
                                                                            value={item.texture_scale}
                                                                            onChange={v => changeItem(i => {
                                                                                i.texture_scale = v;
                                                                            })}/>
                                                                <button
                                                                    onClick={browsePathTexture}>{t("Path texture")}</button>
                                                                <button onClick={() => changeItem(i => {
                                                                    if (i.kind.type === "path") {
                                                                        i.kind.points.reverse();
                                                                        const s = {...defaultPathStyle(), ...i.path_style};
                                                                        i.path_style = {
                                                                            ...s,
                                                                            transition_in: s.transition_out,
                                                                            transition_out: s.transition_in
                                                                        };
                                                                    }
                                                                })}>{t("Reverse path direction")}</button>
                                                            </div>}
                                                        {item.kind.type === "roof" &&
                                                            <div className="geometry-controls">
                                                                <RoofFields t={t} value={item.kind} onChange={v => {
                                                                    const next = structuredClone(item);
                                                                    if (next.kind.type === "roof") Object.assign(next.kind, v);
                                                                    commitGeometry(next);
                                                                }}/>
                                                                <RangeField label={t("Texture scale")} unit={t("cells")}
                                                                            min={0.1} max={64} step={0.25}
                                                                            value={item.texture_scale}
                                                                            onChange={v => changeItem(i => {
                                                                                i.texture_scale = v;
                                                                            })}/>
                                                                <button onClick={() => {
                                                                    setCategory("roofs");
                                                                    setQuery("");
                                                                    setTab("library");
                                                                    setRight(true);
                                                                }}>{t("Roof style")}</button>
                                                                <button onClick={() => changeItem(i => {
                                                                    if (i.kind.type === "roof") i.kind.points.reverse();
                                                                })}>{t("Reverse roof direction")}</button>
                                                            </div>}
                                                        {supportsGeometry(item) && <div className="geometry-controls">
                                                            <span className="muted tiny">{t("Geometry")}</span>
                                                            <div className="geometry-actions">
                                                                <button
                                                                    className={activeGeometryMode === "points" ? "active" : ""}
                                                                    onClick={() => {
                                                                        setGeometryMode(geometryMode === "points" ? null : "points");
                                                                        setTool("select");
                                                                        setGesture([]);
                                                                    }}>{t("Edit points")}</button>
                                                                {item.kind.type !== "roof" && item.kind.type !== "path" &&
                                                                    <button
                                                                        className={geometryMode === "resize" ? "active" : ""}
                                                                        onClick={() => {
                                                                            setGeometryMode(geometryMode === "resize" ? null : "resize");
                                                                            setTool("select");
                                                                            setGesture([]);
                                                                        }}>{t("Resize")}</button>}
                                                            </div>
                                                            {item.kind.type !== "roof" && item.kind.type !== "path" &&
                                                                <div className="two-fields">
                                                                    <NumberField label={t("Shape width")}
                                                                                 value={bounds(item).right - bounds(item).x}
                                                                                 min={0.05} max={4096} step={0.5}
                                                                                 disabled={bounds(item).right === bounds(item).x}
                                                                                 onChange={w => {
                                                                                     const next = structuredClone(item);
                                                                                     resizeGeometry(next, w, bounds(item).bottom - bounds(item).y);
                                                                                     commitGeometry(next);
                                                                                 }}/>
                                                                    <NumberField label={t("Height")}
                                                                                 value={bounds(item).bottom - bounds(item).y}
                                                                                 min={0.05} max={4096} step={0.5}
                                                                                 disabled={bounds(item).bottom === bounds(item).y}
                                                                                 onChange={h => {
                                                                                     const next = structuredClone(item);
                                                                                     resizeGeometry(next, bounds(item).right - bounds(item).x, h);
                                                                                     commitGeometry(next);
                                                                                 }}/>
                                                                </div>
                                                            }
                                                            {activeGeometryMode === "points" && <>
                                                                <p className="muted tiny">{t("Geometry hint")}</p>
                                                                {vertex && geometryRings(item)[vertex.ring]?.[vertex.point] && <>
                                                                    <div className="two-fields">
                                                                        {(["x", "y"] as const).map(axis => <NumberField
                                                                            key={axis} label={axis.toUpperCase()}
                                                                            value={geometryRings(item)[vertex.ring][vertex.point][axis]}
                                                                            min={-4096} max={4096} step={0.5}
                                                                            onChange={value => {
                                                                                const next = structuredClone(item);
                                                                                moveVertex(next, vertex, {
                                                                                    ...geometryRings(item)[vertex.ring][vertex.point],
                                                                                    [axis]: value
                                                                                });
                                                                                commitGeometry(next);
                                                                            }}/>)}
                                                                    </div>
                                                                    <button onClick={() => {
                                                                        const next = structuredClone(item);
                                                                        if (removeVertex(next, vertex)) {
                                                                            commitGeometry(next);
                                                                            setVertex(null);
                                                                        }
                                                                    }}>{t("Delete point")}</button>
                                                                </>}
                                                            </>}
                                                            {item.kind.type !== "wall" && item.kind.type !== "roof" && item.kind.type !== "path" && <>
                                                                <button
                                                                    className={geometryMode === "hole" ? "active" : ""}
                                                                    onClick={() => {
                                                                        setGeometryMode(geometryMode === "hole" ? "points" : "hole");
                                                                        setTool("select");
                                                                        setGesture([]);
                                                                    }}>{t("Draw hole")}</button>
                                                                {geometryMode === "hole" && <>
                                                                    <div className="water-shapes" role="group"
                                                                         aria-label={t("Hole shape")}>
                                                                        {([["rectangle", Square, "Rectangle"], ["circle", Circle, "Circle"], ["polygon", Pentagon, "Polygon"]] as const).map(([shape, Icon, label]) =>
                                                                            <IconButton key={shape} label={t(label)}
                                                                                        active={holeShape === shape}
                                                                                        onClick={() => {
                                                                                            setHoleShape(shape);
                                                                                            setGesture([]);
                                                                                        }}><Icon
                                                                                size={18}/></IconButton>)}
                                                                    </div>
                                                                    {holeShape === "circle" &&
                                                                        <RangeField label={t("Segmentation")} unit=""
                                                                                    value={segmentation} min={6}
                                                                                    max={64} step={1}
                                                                                    onChange={setSegmentation}/>}
                                                                    {item.kind.type === "room" &&
                                                                        <Toggle label={t("Hole walls")}
                                                                                checked={holeWalls}
                                                                                onChange={setHoleWalls}/>}
                                                                    <p className="muted tiny">{t("Hole hint")}</p>
                                                                    {holeShape === "polygon" &&
                                                                        <button disabled={gesture.length < 3}
                                                                                onClick={finishShape}>{t("Finish shape")}</button>}
                                                                </>}
                                                                {geometryRings(item).map((ring, n) => {
                                                                    const hole = item.kind.type === "floor" ? n > 0 : signedArea(ring) < 0;
                                                                    return <div key={n} className="contour-controls">
                                                                        <span
                                                                            className="muted tiny">{t(hole ? "Hole" : "Outer contour")} {n + 1}</span>
                                                                        {item.kind.type === "room" &&
                                                                            <Toggle label={t("Contour walls")}
                                                                                    checked={!(item.wall_exclusions ?? []).includes(n)}
                                                                                    onChange={enabled => changeItem(i => setContourWalls(i, n, enabled))}/>}
                                                                        {hole && <button onClick={() => {
                                                                            const next = structuredClone(item);
                                                                            removeHole(next, n);
                                                                            commitGeometry(next);
                                                                            setVertex(null);
                                                                        }}>{t("Delete hole")}</button>}
                                                                    </div>;
                                                                })}
                                                            </>}
                                                        </div>}
                                                        {"position" in item.kind && (
                                                            <div className="two-fields">
                                                                <NumberField
                                                                    label="X"
                                                                    value={item.kind.position.x}
                                                                    min={-512}
                                                                    onChange={(x) =>
                                                                        changeItem((i) => {
                                                                            if ("position" in i.kind)
                                                                                i.kind.position.x = x;
                                                                        })
                                                                    }
                                                                />
                                                                <NumberField
                                                                    label="Y"
                                                                    value={item.kind.position.y}
                                                                    min={-512}
                                                                    onChange={(y) =>
                                                                        changeItem((i) => {
                                                                            if ("position" in i.kind)
                                                                                i.kind.position.y = y;
                                                                        })
                                                                    }
                                                                />
                                                            </div>
                                                        )}
                                                        {item.kind.type === "object" && (
                                                            <>
                                                                <Toggle label={t("Block light")}
                                                                        checked={item.blocks_light ?? false}
                                                                        onChange={value => changeItem(i => {
                                                                            i.blocks_light = value;
                                                                        })}/>
                                                                <div className="two-fields">
                                                                    <NumberField
                                                                        label={t("Width")}
                                                                        value={item.kind.size.x}
                                                                        min={0.01}
                                                                        step={0.1}
                                                                        onChange={(x) =>
                                                                            changeItem((i) => {
                                                                                setObjectSize(i, "x", x);
                                                                            })
                                                                        }
                                                                    />
                                                                    <NumberField
                                                                        label={t("Size")}
                                                                        value={item.kind.size.y}
                                                                        min={0.01}
                                                                        step={0.1}
                                                                        onChange={(y) =>
                                                                            changeItem((i) => {
                                                                                setObjectSize(i, "y", y);
                                                                            })
                                                                        }
                                                                    />
                                                                </div>
                                                                <NumberField
                                                                    label={t("Rotation")}
                                                                    value={(item.kind.rotation * 180) / Math.PI}
                                                                    min={-360}
                                                                    max={360}
                                                                    step={15}
                                                                    onChange={(r) =>
                                                                        changeItem((i) => {
                                                                            if (i.kind.type === "object")
                                                                                i.kind.rotation = (r * Math.PI) / 180;
                                                                        })
                                                                    }
                                                                />
                                                                <div className="selection-actions">
                                                                    <IconButton
                                                                        label={t("Flip horizontal")}
                                                                        active={item.kind.flip_x}
                                                                        onClick={() =>
                                                                            changeItem((i) => {
                                                                                if (i.kind.type === "object")
                                                                                    i.kind.flip_x = !i.kind.flip_x;
                                                                            })
                                                                        }
                                                                    >
                                                                        <FlipHorizontal size={17}/>
                                                                    </IconButton>
                                                                    <IconButton
                                                                        label={t("Flip vertical")}
                                                                        active={item.kind.flip_y}
                                                                        onClick={() =>
                                                                            changeItem((i) => {
                                                                                if (i.kind.type === "object")
                                                                                    i.kind.flip_y = !i.kind.flip_y;
                                                                            })
                                                                        }
                                                                    >
                                                                        <FlipVertical size={17}/>
                                                                    </IconButton>
                                                                </div>
                                                            </>
                                                        )}
                                                        {item.cave ?
                                                            <CaveFields value={item.cave} t={t} styles={caveMaterials}
                                                                        material={caveMaterials.find(s => s.floor.meta.id === item.texture)?.id ?? ""}
                                                                        onChange={cave => changeItem(i => {
                                                                            i.cave = cave;
                                                                        })}
                                                                        onMaterial={s => changeItem(i => {
                                                                            i.texture = s.floor.meta.id;
                                                                            i.border = s.wall.meta.id;
                                                                            i.texture_scale = s.floor.meta.size.x;
                                                                            if (i.cave) i.cave.colorable = s.colorable;
                                                                        })}/>
                                                            : item.water ? <WaterFields value={item.water} t={t}
                                                                                        onChange={water => changeItem(i => setWaterStyle(i, water))}/> : item.kind.type === "room" ? (
                                                                <>
                                                                    {(["tilesets", "walls"] as const).map((part) => {
                                                                        const materialId =
                                                                            item.kind.type === "room"
                                                                                ? part === "tilesets"
                                                                                    ? item.kind.floor
                                                                                    : item.kind.wall
                                                                                : null;
                                                                        const material = assets.find(
                                                                            (a) => a.meta.id === materialId,
                                                                        );
                                                                        return (
                                                                            <label key={part} className="field">
                                    <span>
                                      {t(
                                          part === "tilesets"
                                              ? "Floor material"
                                              : "Wall material",
                                      )}
                                    </span>
                                                                                <button
                                                                                    className="room-material-picker"
                                                                                    aria-label={t(
                                                                                        part === "tilesets"
                                                                                            ? "Floor material"
                                                                                            : "Wall material",
                                                                                    )}
                                                                                    onClick={() => browseRoomMaterial(part)}
                                                                                >
                                                                                    {material ? (
                                                                                        <Thumbnail id={material.meta.id}
                                                                                                   color={roomTint(item, part === "tilesets")}/>
                                                                                    ) : (
                                                                                        <Layers size={20}/>
                                                                                    )}
                                                                                    <span>
                                        {material?.meta.name.replaceAll(
                                                "_",
                                                " ",
                                            ) ??
                                            (materialId ||
                                                t(
                                                    part === "tilesets"
                                                        ? "No floor"
                                                        : "No material",
                                                ))}
                                      </span>
                                                                                    <ChevronRight size={15}/>
                                                                                </button>
                                                                            </label>
                                                                        );
                                                                    })}
                                                                    {colorField(
                                                                        "Floor color",
                                                                        hex(roomTint(item, true)),
                                                                        (c) =>
                                                                            changeItem((i) => {
                                                                                setRoomTint(i, true, rgba(c));
                                                                            }),
                                                                    )}
                                                                    {colorField(
                                                                        "Wall color",
                                                                        hex(roomTint(item, false)),
                                                                        (c) =>
                                                                            changeItem((i) => {
                                                                                setRoomTint(i, false, rgba(c));
                                                                            }),
                                                                    )}
                                                                </>
                                                            ) : item.kind.type === "light" || item.kind.type === "text" ? null : (
                                                                colorField(
                                                                    "Color",
                                                                    hex(
                                                                        "color" in item.kind
                                                                            ? item.kind.color
                                                                            : item.tint,
                                                                    ),
                                                                    (c) =>
                                                                        changeItem((i) => {
                                                                            i.tint = rgba(c);
                                                                            if ("color" in i.kind) i.kind.color = rgba(c);
                                                                        }),
                                                                )
                                                            )}
                                                        {"width" in item.kind && item.kind.type !== "path" && (
                                                            <NumberField
                                                                label={t("Width")}
                                                                value={item.kind.width}
                                                                min={0.01}
                                                                max={5}
                                                                step={0.01}
                                                                onChange={(w) =>
                                                                    changeItem((i) => {
                                                                        if ("width" in i.kind) i.kind.width = w;
                                                                    })
                                                                }
                                                            />
                                                        )}
                                                        {item.kind.type === "light" && (
                                                            <>
                                                                <LightFields value={item.kind} t={t}
                                                                             onChange={value => changeItem(i => {
                                                                                 if (i.kind.type === "light") Object.assign(i.kind, value);
                                                                             })}/>
                                                                <Toggle
                                                                    label={t("Shadows")}
                                                                    checked={item.kind.shadows}
                                                                    onChange={(v) =>
                                                                        changeItem((i) => {
                                                                            if (i.kind.type === "light")
                                                                                i.kind.shadows = v;
                                                                        })
                                                                    }
                                                                />
                                                            </>
                                                        )}
                                                        {item.kind.type === "portal" && (
                                                            <>
                                                                <label className="field">
                                                                    <span>{t("Portal style")}</span>
                                                                    <button className="room-material-picker"
                                                                            aria-label={t("Portal style")}
                                                                            onClick={() => {
                                                                                setCategory("portals");
                                                                                setQuery("");
                                                                                setFavoriteOnly(false);
                                                                                setTab("library");
                                                                                setRight(true);
                                                                            }}>
                                                                        {item.texture ? <Thumbnail id={item.texture}
                                                                                                   color={item.tint}/> :
                                                                            <DoorOpen size={20}/>}
                                                                        <span>{assets.find(a => a.meta.id === item.texture)?.meta.name.replaceAll("_", " ") ?? item.texture ?? t("No material")}</span>
                                                                        <ChevronRight size={15}/>
                                                                    </button>
                                                                </label>
                                                                <label className="field">
                                                                    <span>{t("Portal type")}</span>
                                                                    <select
                                                                        value={portalType(item, assets.find(a => a.meta.id === item.texture)?.meta)}
                                                                        onChange={(event) => {
                                                                            const value = event.target.value === "window" ? "window" : "door";
                                                                            changeItem(i => {
                                                                                if (i.kind.type === "portal") {
                                                                                    i.portal_type = value;
                                                                                    i.kind.closed = value === "door";
                                                                                }
                                                                            });
                                                                        }}>
                                                                        <option value="door">{t("Door")}</option>
                                                                        <option value="window">{t("Window")}</option>
                                                                    </select>
                                                                </label>
                                                            </>
                                                        )}
                                                        {!item.water && ["room", "floor", "terrain_region"].includes(item.kind.type) && (
                                                            <RangeField
                                                                label={t(item.kind.type === "terrain_region" ? "Texture scale" : "Floor texture scale")}
                                                                unit={t("cells")}
                                                                value={item.texture_scale}
                                                                onChange={value => changeItem(i => {
                                                                    i.texture_scale = value;
                                                                })}
                                                            />
                                                        )}
                                                        {item.kind.type === "text" && (
                                                            tool === "text" ?
                                                                <p className="muted tiny">{t("Label editing hint")}</p> :
                                                                <TextEditor key={item.id} value={item.kind}
                                                                            style={item.text_style}
                                                                            ppg={project.pixels_per_grid}
                                                                            mode={textMode} t={t}
                                                                            onMode={mode => {
                                                                                setTextMode(mode);
                                                                                setTool("text");
                                                                            }}
                                                                            onChange={patch => changeItem(i => {
                                                                                if (i.kind.type === "text") i.kind = {...i.kind, ...patch};
                                                                                i.tint = white;
                                                                            })}
                                                                            onStyle={style => changeItem(i => {
                                                                                i.text_style = style;
                                                                                i.tint = white;
                                                                            })}/>
                                                        )}
                                                    </fieldset>
                                                    <Toggle
                                                        label={t("Locked")}
                                                        checked={item.locked}
                                                        onChange={(locked) =>
                                                            void edit((p) => {
                                                                const i = p.levels[level].layers
                                                                    .flatMap((l) => l.items)
                                                                    .find((i) => i.id === item.id);
                                                                if (i) i.locked = locked;
                                                            })
                                                        }
                                                    />
                                                </>
                                            )}
                                        </>
                                    )}
                                    <details className="layer-object-list" open>
                                        <summary>{t("Items in layer")} · {t(activeLayer.name)}</summary>
                                        {[...activeLayer.items].reverse().map((i, n) => (
                                            <button key={i.id} className={selected.includes(i.id) ? "selected" : ""}
                                                    onClick={e => {
                                                        setSelected(e.ctrlKey || e.metaKey ? selected.includes(i.id) ? selected.filter(id => id !== i.id) : [...selected, i.id] : [i.id]);
                                                    }}>
                                                <span>{activeLayer.items.length - n}</span>
                                                <span>{i.name}</span>
                                                {i.locked && <LockKeyhole size={13}/>}
                                            </button>
                                        ))}
                                    </details>
                                </div>
                            )}
                        </aside>
                    )}
                </div>
            </> : <main className="welcome-screen" data-theme-part="welcome">
                {data.recovery && !recoveryDismissed && <section className="recovery-card" role="status"
                                                                 aria-label={t("Recovery available")}>
                    <History size={23} aria-hidden="true"/>
                    <div className="recovery-card-content">
                        <strong>{t("Recovery available")}</strong>
                        <p>{t("Recovery description")}</p>
                        <div className="recovery-card-details">
                            <span>{data.recovery.name}</span>
                            <time dateTime={new Date(data.recovery.modified_ms).toISOString()}>
                                {new Date(data.recovery.modified_ms).toLocaleString(data.preferences.locale)}
                            </time>
                        </div>
                        <button className="button primary" disabled={!!busy}
                                onClick={() => void run({type: "recover"})}>{t("Recover")}</button>
                    </div>
                    <button className="icon-button" aria-label={t("Close")} title={t("Close")}
                            onClick={() => setRecoveryDismissed(true)}><X size={18}/></button>
                </section>}
                <div className="welcome-brand"><img src={dark ? logoDark : logo} alt="Maploom"/></div>
                <h1>{t("Welcome title")}</h1>
                <p>{t("Welcome description")}</p>
                <div className="welcome-actions">
                    <button className="button primary" disabled={!!busy} onClick={newDialog}><FilePlus2
                        size={19}/>{t("Create new project")}</button>
                    <button className="button subtle" disabled={!!busy}
                            onClick={() => void run({type: "open", path: null})}><FolderOpen
                        size={19}/>{t("Open project")}</button>
                </div>
                <button className="welcome-settings text-button" onClick={() => setModal("settings")}><Settings2
                    size={16}/>{t("Settings")}</button>
            </main>}
            {toast && (
                <div className="toast" role="status">
                    <CheckCheck size={17}/>
                    {t(toast)}
                </div>
            )}
            {busy > 0 && (
                <div className="working-indicator" role="status">
                    <LoaderCircle className="spin" size={15}/>
                    {t("Processing")}
                </div>
            )}
            {error && (
                <div className="error-toast" role="alert">
                    <AlertCircle size={19}/>
                    <div>
                        <strong>{t("Error")}</strong>
                        <p>{t(error)}</p>
                    </div>
                    <IconButton label={t("Close")} onClick={() => setError("")}>
                        <X size={16}/>
                    </IconButton>
                </div>
            )}
            {modal && (
                <div
                    className="modal-backdrop"
                    onPointerDown={(e) => {
                        if (e.target === e.currentTarget) setModal(null);
                    }}
                >
                    <section
                        data-theme-part="dialog"
                        className={`modal modal-${modal}`}
                        role="dialog"
                        aria-modal="true"
                        aria-label={t(
                            modal === "resize-map" ? "Change map size" : modal === "map-settings"
                                ? "Map settings"
                                : modal === "new"
                                    ? "New"
                                    : modal === "export"
                                        ? "Export"
                                        : modal === "commands"
                                            ? "Commands"
                                            : modal === "catalogs"
                                                ? "Asset catalogs"
                                                : modal === "asset"
                                                    ? "Edit asset"
                                                    : modal[0].toUpperCase() + modal.slice(1),
                        )}
                    >
                        {modal === "resize-map" && <>
                            {modalHeader("Change map size")}
                            <MapResize width={project.width} height={project.height} t={t} busy={!!busy}
                                       onCancel={() => setModal("map-settings")}
                                       onApply={edges => {
                                           void run({type: "resize_map", ...edges}).then(ok => {
                                               if (ok) {
                                                   setGesture([]);
                                                   setGeometryDraft(null);
                                                   setSelected([]);
                                                   setModal("map-settings");
                                               }
                                           });
                                       }}/>
                        </>}
                        {modal === "map-settings" && (
                            <>
                                {modalHeader("Map settings")}
                                <div className="modal-body map-settings-body">
                                    <div className="map-details">
                                        {textField(
                                            "Name",
                                            project.name,
                                            (name) =>
                                                void edit((p) => {
                                                    p.name = name;
                                                }),
                                        )}
                                        <button className="button subtle" onClick={() => setModal("resize-map")}>
                                            <Maximize size={16}/>{t("Change map size")}</button>
                                        <span className="muted tiny">
              {project.width} × {project.height} {t("cells")} ·{" "}
                                            {allItems.length} {t("items")}
            </span>
                                    </div>
                                    <VariationsPanel project={project} busy={!!busy} t={t} run={run}/>
                                    <FloorsPanel project={project} level={level} busy={!!busy} t={t} edit={edit}
                                                 select={id => {
                                                     const p = current.current!.project;
                                                     const index = p.levels.findIndex(l => l.id === id);
                                                     if (index < 0) return;
                                                     setLevel(index);
                                                     setLayer(Math.min(layer, p.levels[index].layers.length - 1));
                                                     setSelected([]);
                                                 }}/>
                                    <div className="section-heading">
            <span>
              {t("Layers")}
                <small>{activeLevel.layers.length}</small>
            </span>
                                        <IconButton
                                            label={t("Add layer")}
                                            onClick={() =>
                                                void edit((p) => {
                                                    p.levels[level].layers.push(
                                                        newLayer(`${t("Layers")} ${activeLevel.layers.length + 1}`),
                                                    );
                                                }).then(() =>
                                                    setLayer(
                                                        current.current!.project.levels[level].layers.length - 1,
                                                    ),
                                                )
                                            }
                                        >
                                            <Plus size={15}/>
                                        </IconButton>
                                    </div>
                                    <div className="layers-list">
                                        {[...activeLevel.layers].reverse().map((l, n) => {
                                            const index = activeLevel.layers.length - n - 1;
                                            return (
                                                <div
                                                    key={l.id}
                                                    className={`layer-row ${layer === index ? "selected" : ""}`}
                                                >
                                                    <button
                                                        className="layer-select"
                                                        onClick={() => {
                                                            setLayer(index);
                                                            setSelected([]);
                                                        }}
                                                    >
                                                        <Layers size={16}/>
                                                        <span>
                      {t(l.name)}
                                                            <small>
                        {l.items.length} {t("items")}
                      </small>
                    </span>
                                                    </button>
                                                    <IconButton
                                                        label={t("Visible")}
                                                        active={l.visible}
                                                        onClick={() =>
                                                            void edit((p) => {
                                                                p.levels[level].layers[index].visible = !l.visible;
                                                            })
                                                        }
                                                    >
                                                        {l.visible ? <Eye size={14}/> : <EyeOff size={14}/>}
                                                    </IconButton>
                                                    <IconButton
                                                        label={t("Locked")}
                                                        active={l.locked}
                                                        onClick={() =>
                                                            void edit((p) => {
                                                                p.levels[level].layers[index].locked = !l.locked;
                                                            })
                                                        }
                                                    >
                                                        {l.locked ? (
                                                            <LockKeyhole size={14}/>
                                                        ) : (
                                                            <LockKeyholeOpen size={14}/>
                                                        )}
                                                    </IconButton>
                                                </div>
                                            );
                                        })}
                                    </div>
                                    <div className="layer-settings">
                                        {textField("Name", activeLayer.name, (name) =>
                                            updateLayer((l) => {
                                                l.name = name;
                                            }),
                                        )}
                                        <RangeField
                                            key={activeLayer.id}
                                            label={t("Opacity")}
                                            unit="%"
                                            min={0}
                                            max={100}
                                            step={1}
                                            value={Number((activeLayer.opacity * 100).toFixed(10))}
                                            onChange={(opacity) => updateLayer((l) => {
                                                l.opacity = opacity / 100;
                                            })}
                                        />
                                        <div className="layer-actions">
                                            <IconButton
                                                label={t("Move up")}
                                                disabled={layer === activeLevel.layers.length - 1}
                                                onClick={() => {
                                                    void edit((p) => {
                                                        const a = p.levels[level].layers;
                                                        [a[layer], a[layer + 1]] = [a[layer + 1], a[layer]];
                                                    });
                                                    setLayer(layer + 1);
                                                }}
                                            >
                                                <ArrowUp size={15}/>
                                            </IconButton>
                                            <IconButton
                                                label={t("Move down")}
                                                disabled={layer === 0}
                                                onClick={() => {
                                                    void edit((p) => {
                                                        const a = p.levels[level].layers;
                                                        [a[layer], a[layer - 1]] = [a[layer - 1], a[layer]];
                                                    });
                                                    setLayer(layer - 1);
                                                }}
                                            >
                                                <ArrowDown size={15}/>
                                            </IconButton>
                                            <IconButton
                                                label={t("Delete")}
                                                disabled={
                                                    activeLevel.layers.length === 1 ||
                                                    activeLayer.locked ||
                                                    activeLayer.items.some((i) => i.locked)
                                                }
                                                onClick={() => {
                                                    void edit((p) => {
                                                        p.levels[level].layers.splice(layer, 1);
                                                    });
                                                    setLayer(Math.max(0, layer - 1));
                                                }}
                                            >
                                                <Trash2 size={15}/>
                                            </IconButton>
                                        </div>
                                    </div>

                                    <Toggle
                                        label={t("Default assets")}
                                        checked={project.use_default_assets}
                                        onChange={(v) => {
                                            void edit((p) => {
                                                p.use_default_assets = v;
                                            });
                                        }}
                                    />
                                    {colorField(
                                        "Background",
                                        hex(project.background),
                                        (c) =>
                                            void edit((p) => {
                                                p.background = rgba(c);
                                                p.background_asset = null;
                                            }),
                                    )}

                                </div>
                            </>
                        )}
                        {modal === "new" && (
                            <>
                                {modalHeader("New", "New description")}
                                <ProjectSetup t={t} busy={!!busy} presets={data.preferences.map_size_presets ?? []}
                                              onPresets={map_size_presets => pref({map_size_presets})}
                                              onCancel={() => setModal(null)}
                                              onCreate={(name, width, height) => closeAfter(run({
                                                  type: "new",
                                                  name,
                                                  width,
                                                  height,
                                                  demo: false
                                              }))}
                                              onDemo={() => closeAfter(run({
                                                  type: "new",
                                                  name: "The Mosslight Sanctuary",
                                                  width: 32,
                                                  height: 24,
                                                  demo: true
                                              }))}/>
                            </>
                        )}
                        {modal === "export" && (
                            <>
                                {modalHeader("Export", "Export description")}
                                <div className="modal-body">
                                    <Toggle label={t("Export all floors")} checked={exportAll} onChange={setExportAll}/>
                                    <Toggle label={t("Export all variations")} checked={exportVariations}
                                            onChange={setExportVariations}/>
                                    {exportVariations &&
                                        <p className="info-note">{t(exportAll ? "Variation floors export hint" : "Variations export hint")}</p>}
                                    <div className="export-formats">
                                        {["png", "webp", "jpeg", "svg", "dd2vtt"].map((f) => (
                                            <button
                                                key={f}
                                                disabled={f === "jpeg" && exportNeedsAlpha}
                                                title={f === "jpeg" && exportNeedsAlpha ? t("Floor transparency requires PNG, WebP, SVG or VTT") : undefined}
                                                className={f === exportFormat ? "active" : ""}
                                                onClick={() => setExportFormat(f)}
                                            >
                                                <span>{f === "dd2vtt" ? "VTT" : f.toUpperCase()}</span>
                                                <small>
                                                    {f === "dd2vtt"
                                                        ? "DD2VTT 0.3"
                                                        : f === "svg"
                                                            ? "VECTOR"
                                                            : f === "jpeg"
                                                                ? "JPEG"
                                                                : "IMAGE"}
                                                </small>
                                                {f === exportFormat && <Check size={14}/>}
                                            </button>
                                        ))}
                                    </div>
                                    {!exportAll && !exportVariations && <label className="field">
                                        <span>{t("Levels")}</span>
                                        <select
                                            value={level}
                                            onChange={(e) => setLevel(+e.target.value)}
                                        >
                                            {project.levels.map((l, i) => (
                                                <option value={i} key={l.id}>
                                                    {t(l.name)}
                                                </option>
                                            ))}
                                        </select>
                                    </label>}
                                    {exportAll && !exportVariations &&
                                        <p className="info-note">{t("Batch export hint")} · {project.levels.length} {t("Levels")}</p>}
                                    <NumberField
                                        label={t("Resolution")}
                                        value={ppg}
                                        min={16}
                                        max={512}
                                        onChange={setPpg}
                                    />
                                    <div className="output-summary">
                                        <Download size={22}/>
                                        <div>
                                            <span>{t("Output")}</span>
                                            <strong>
                                                {project.width * ppg} × {project.height * ppg} px
                                            </strong>
                                        </div>
                                        <small>
                                            {(
                                                (project.width * project.height * ppg * ppg) /
                                                1e6
                                            ).toFixed(1)}{" "}
                                            MP
                                        </small>
                                    </div>
                                    <Toggle
                                        label={t("Grid")}
                                        checked={exportGrid}
                                        onChange={setExportGrid}
                                    />
                                    <Toggle
                                        label={t("Lights")}
                                        checked={exportLights}
                                        onChange={setExportLights}
                                    />
                                    {exportFormat === "jpeg" && (
                                        <NumberField
                                            label={t("Quality")}
                                            value={quality}
                                            min={1}
                                            max={100}
                                            onChange={setQuality}
                                        />
                                    )}
                                    <p className="info-note">
                                        {exportFormat === "dd2vtt"
                                            ? t("VTT hint")
                                            : t("Export description")}
                                    </p>
                                    {exportFormat === "dd2vtt" && (
                                        <p className="info-note">{t("VTT windows hint")}</p>
                                    )}
                                    <p className="info-note">{t("Floor export backgrounds hint")}</p>
                                </div>
                                <div className="modal-footer">
                                    <button
                                        className="button subtle"
                                        onClick={() => setModal(null)}
                                    >
                                        {t("Cancel")}
                                    </button>
                                    <button
                                        className="button primary"
                                        disabled={
                                            !!busy ||
                                            (exportNeedsAlpha && exportFormat === "jpeg") ||
                                            (project.width * project.height * ppg * ppg > 64000000 &&
                                                exportFormat !== "svg")
                                        }
                                        onClick={() =>
                                            closeAfter(
                                                run(
                                                    {
                                                        type: exportVariations ? "export_variations" : exportAll ? "export_all" : "export",
                                                        floors: exportAll,
                                                        level,
                                                        format: exportFormat,
                                                        pixels_per_grid: ppg,
                                                        grid: exportGrid,
                                                        lights: exportLights,
                                                        quality,
                                                    },
                                                    "Exported",
                                                ),
                                            )
                                        }
                                    >
                                        <Download size={16}/>
                                        {t(exportVariations ? "Export all variations" : exportAll ? "Export all floors" : "Export")}
                                    </button>
                                </div>
                            </>
                        )}
                        {modal === "catalogs" && (
                            <>
                                {modalHeader("Asset catalogs")}
                                <div className="modal-body">
                                    {duplicatePacks.length > 0 && <div className="info-note catalog-duplicates">
                                        <p>{t("Duplicate packs hint")}</p>
                                        {duplicatePacks.map(paths => <ul key={paths[0]}>{paths.map(path => <li
                                            key={path}>{path}</li>)}</ul>)}
                                    </div>}
                                    <div className="catalog-toolbar">
                                        <button className="button primary" disabled={!!busy} onClick={() => void run({
                                            type: "add_directory",
                                            path: null,
                                            id: null
                                        }, "Catalogs updated")}>
                                            <FolderOpen size={16}/>{t("Add directory")}
                                        </button>
                                        <button className="button" disabled={!!busy}
                                                onClick={() => void run({type: "refresh_directories"}, "Catalogs updated")}>
                                            <RotateCw size={16}/>{t("Rescan directories")}
                                        </button>
                                    </div>
                                    {!catalogs.length && <p className="info-note">{t("No asset directories")}</p>}
                                    {project.asset_catalogs.filter(ref => !catalogs.some(c => c.directory.id === ref.id)).map(ref => (
                                        <section className="catalog-card" key={ref.id}>
                                            <strong>{ref.name}</strong><p>{t("Missing directory")}</p>
                                            <button className="button" disabled={!!busy} onClick={() => void run({
                                                type: "add_directory",
                                                path: null,
                                                id: ref.id
                                            }, "Catalogs updated")}>{t("Locate directory")}</button>
                                            <button className="button subtle" disabled={!!busy}
                                                    onClick={() => void edit(p => {
                                                        p.asset_catalogs = p.asset_catalogs.filter(c => c.id !== ref.id);
                                                    })}>{t("Disable for this project")}</button>
                                        </section>
                                    ))}
                                    {catalogs.map(catalog => (
                                        <details className="catalog-card" key={catalog.directory.id}>
                                            <summary className="catalog-summary">
                                                <input type="checkbox"
                                                       aria-label={`${t("Enable entire catalog")}: ${catalog.directory.name}`}
                                                       title={t("Enable entire catalog")}
                                                       disabled={!!busy}
                                                       checked={catalogSelectionState(project, catalog) === "all"}
                                                       ref={input => {
                                                           if (input) input.indeterminate = catalogSelectionState(project, catalog) === "partial";
                                                       }}
                                                       onClick={e => e.stopPropagation()} onChange={e => {
                                                    const enabled = e.target.checked;
                                                    void edit(p => setCatalogEnabled(p, catalogs, catalog, enabled));
                                                }}/>
                                                <strong>{catalog.directory.name}</strong></summary>
                                            <p className="catalog-path">{displayDirectoryPath(catalog.directory.path)}</p>
                                            <div className="catalog-toolbar">
                                                <span>{catalog.assets} {t("items")} · {catalog.packs.length} {t("packs")}</span>
                                                <button className="button subtle compact" disabled={!!busy}
                                                        onClick={() => void run({
                                                            type: "add_directory",
                                                            path: null,
                                                            id: catalog.directory.id
                                                        }, "Catalogs updated")}>{t("Locate directory")}</button>
                                                <button className="button subtle compact" disabled={!!busy}
                                                        onClick={() => void run({
                                                            type: "remove_directory",
                                                            id: catalog.directory.id
                                                        }, "Catalogs updated")}>{t("Remove directory")}</button>
                                            </div>
                                            <div className="catalog-packs">
                                                {catalog.packs.map(pack => (
                                                    <details className="pack-card" key={pack.key}>
                                                        <summary className="pack-summary">
                                                            <input type="checkbox"
                                                                   aria-label={`${t("Enabled for this project")}: ${pack.name}`}
                                                                   disabled={!!busy}
                                                                   checked={catalogPackEnabled(project, catalog, pack.key)}
                                                                   onClick={e => e.stopPropagation()} onChange={e => {
                                                                const enabled = e.target.checked;
                                                                void edit(p => setCatalogPack(p, catalogs, catalog, pack.key, enabled));
                                                            }}/>
                                                            <strong>{pack.name}</strong>
                                                        </summary>
                                                        <div className="pack-content">
                                                            {pack.metadata.logo ?
                                                                <img className="pack-logo" src={pack.metadata.logo}
                                                                     alt={pack.name} loading="lazy"/> :
                                                                <div className="pack-logo pack-placeholder"><Package
                                                                    size={30}/></div>}
                                                            <div className="pack-details">
                                                                <p>{[pack.metadata.author, pack.metadata.version && `${t("Version")} ${pack.metadata.version}`].filter(Boolean).join(" · ")}</p>
                                                                {pack.metadata.description &&
                                                                    <p className="pack-description">{pack.metadata.description}</p>}
                                                                {pack.metadata.website &&
                                                                    <p>{pack.metadata.website}</p>}
                                                                <small>{pack.assets} {t("items")}</small>
                                                            </div>
                                                        </div>
                                                    </details>
                                                ))}
                                                {catalog.loose_images > 0 && <details className="pack-card">
                                                    <summary className="pack-summary">
                                                        <input type="checkbox"
                                                               aria-label={`${t("Enabled for this project")}: ${t("Loose images")}`}
                                                               disabled={!!busy}
                                                               checked={catalogPackEnabled(project, catalog, "")}
                                                               onClick={e => e.stopPropagation()} onChange={e => {
                                                            const enabled = e.target.checked;
                                                            void edit(p => setCatalogPack(p, catalogs, catalog, "", enabled));
                                                        }}/>
                                                        <strong>{t("Loose images")}</strong>
                                                    </summary>
                                                    <div className="pack-content">
                                                        <small>{catalog.loose_images} {t("items")}</small></div>
                                                </details>}
                                            </div>
                                        </details>
                                    ))}
                                </div>
                            </>
                        )}
                        {modal === "settings" && (
                            <>
                                {modalHeader("Settings")}
                                <div className="settings-tabs" role="tablist" aria-label={t("Settings")}>
                                    {(["appearance", "localization", "updates", "general"] as const).map((tab, index, tabs) =>
                                        <button key={tab} id={`settings-tab-${tab}`}
                                                role="tab" aria-selected={settingsTab === tab}
                                                aria-controls={`settings-panel-${tab}`}
                                                tabIndex={settingsTab === tab ? 0 : -1}
                                                onKeyDown={e => {
                                                    if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) {
                                                        e.preventDefault();
                                                        const next = e.key === "Home" ? tabs[0] : e.key === "End" ? tabs[tabs.length - 1] : tabs[(index + (e.key === "ArrowRight" ? 1 : tabs.length - 1)) % tabs.length];
                                                        setSettingsTab(next);
                                                        document.getElementById(`settings-tab-${next}`)?.focus();
                                                    }
                                                }}
                                                onClick={() => setSettingsTab(tab)}>{t({
                                            appearance: "Settings appearance",
                                            localization: "Settings localization",
                                            updates: "Updates",
                                            general: "Settings general"
                                        }[tab])}
                                            {tab === "updates" && updates.info?.status === "available" &&
                                                <span className="settings-update-dot"/>}
                                        </button>)}
                                </div>
                                {settingsTab !== "updates" ?
                                    <div className="modal-body" role="tabpanel" id={`settings-panel-${settingsTab}`}
                                         aria-labelledby={`settings-tab-${settingsTab}`}>
                                        {settingsTab === "appearance" && <>
                                            <ThemeSettings themes={themes} selected={data.preferences.theme}
                                                           selectedVariant={data.preferences.theme_variant}
                                                           dark={data.preferences.dark} accent={data.preferences.accent}
                                                           onSelect={(id, dark, variant) => void pref({
                                                               theme: id,
                                                               theme_variant: variant ?? null, ...(dark === undefined ? {} : {dark})
                                                           })}
                                                           t={t}/>
                                            <div className="setting-section">
                                                <h3>{t("Accent")}</h3>
                                                <div className="accent-swatches">
                                                    {[
                                                        [137, 160, 109],
                                                        [120, 149, 215],
                                                        [171, 140, 205],
                                                        [215, 163, 103],
                                                        [96, 173, 166],
                                                    ].map((c) => (
                                                        <button
                                                            key={c[0]}
                                                            aria-label={`rgb(${c.join(",")})`}
                                                            className={
                                                                c.join() === data.preferences.accent.join()
                                                                    ? "active"
                                                                    : ""
                                                            }
                                                            style={{background: `rgb(${c.join(",")})`}}
                                                            onClick={() =>
                                                                void pref({accent: c as [number, number, number]})
                                                            }
                                                        >
                                                            {c.join() === data.preferences.accent.join() && (
                                                                <Check size={16}/>
                                                            )}
                                                        </button>
                                                    ))}
                                                </div>
                                            </div>
                                        </>}
                                        {settingsTab === "localization" && <label className="field">
                                            <span>{t("Language")}</span>
                                            <select
                                                value={data.preferences.locale}
                                                onChange={(e) => void pref({locale: e.target.value})}
                                            >
                                                {(
                                                    data.locales ?? [
                                                        {id: "ru", name: "Русский"},
                                                        {id: "en", name: "English"},
                                                    ]
                                                ).map((l) => (
                                                    <option key={l.id} value={l.id}>
                                                        {l.name}
                                                    </option>
                                                ))}
                                            </select>
                                        </label>}
                                        {settingsTab === "general" && <>
                                            <button className="button" onClick={() => setModal("catalogs")}>
                                                <FolderOpen size={16}/>{t("Asset catalogs")}
                                            </button>
                                            <Toggle label={t("Default assets")}
                                                    checked={data.preferences.default_assets}
                                                    onChange={v => void pref({default_assets: v})}/>
                                            <p className="info-note">{t("Autosave hint")}</p>
                                        </>}
                                    </div> : <div className="modal-body" role="tabpanel" id="settings-panel-updates"
                                                  aria-labelledby="settings-tab-updates">
                                        <UpdatesPanel updates={updates} t={t} dirty={data.dirty}
                                                      busy={busy > 0 || !!gesture.length || !!drag}
                                                      onInstall={async () => {
                                                          if (current.current?.dirty) {
                                                              const saved = await run({
                                                                  type: "save",
                                                                  save_as: false
                                                              }, "Saved toast");
                                                              if (!saved || current.current?.dirty) return;
                                                          }
                                                          await updates.install();
                                                      }}/>
                                    </div>}
                            </>
                        )}
                        {modal === "commands" && (
                            <>
                                <div className="command-input">
                                    <Search size={20}/>
                                    <input
                                        autoFocus
                                        placeholder={t("Command search")}
                                        value={commandQuery}
                                        onChange={(e) => setCommandQuery(e.target.value)}
                                        onKeyDown={(e) => {
                                            if (e.key === "Enter") {
                                                const c = commands.find((c) =>
                                                    t(c.label)
                                                        .toLowerCase()
                                                        .includes(commandQuery.toLowerCase()),
                                                );
                                                if (c) {
                                                    setModal(null);
                                                    c.action();
                                                }
                                            }
                                        }}
                                    />
                                    <kbd>ESC</kbd>
                                </div>
                                <div className="command-results">
                                    <span className="eyebrow">{t("Recent")}</span>
                                    {commands
                                        .filter((c) =>
                                            t(c.label)
                                                .toLowerCase()
                                                .includes(commandQuery.toLowerCase()),
                                        )
                                        .map((c) => (
                                            <button
                                                key={c.label}
                                                onClick={() => {
                                                    setModal(null);
                                                    c.action();
                                                }}
                                            >
                                                <c.icon size={18}/>
                                                <span>{t(c.label)}</span>
                                                <kbd>{c.key}</kbd>
                                            </button>
                                        ))}
                                </div>
                            </>
                        )}
                        {modal === "help" && (
                            <>
                                {modalHeader("Help")}
                                <div className="modal-body shortcut-list">
                                    {commands
                                        .filter((c) => c.key)
                                        .map((c) => (
                                            <div key={c.label}>
                                                <span>{t(c.label)}</span>
                                                <kbd>{c.key}</kbd>
                                            </div>
                                        ))}
                                    <div>
                                        <span>{t("Pan")}</span>
                                        <kbd>Space / MMB</kbd>
                                    </div>
                                    <p className="muted tiny">{t("Object wheel hint")}</p>
                                    <div>
                                        <span>{t("Selection")}</span>
                                        <kbd>Shift + Click</kbd>
                                    </div>
                                    <div>
                                        <span>{t("Delete")}</span>
                                        <kbd>Delete</kbd>
                                    </div>
                                </div>
                            </>
                        )}
                        {modal === "report" && (
                            <>
                                {modalHeader("Report")}
                                <div className="modal-body report-list">
                                    {data.warnings.length ? (
                                        data.warnings.map((w, i) => (
                                            <p key={i}>
                                                <AlertCircle size={16}/>
                                                {w}
                                            </p>
                                        ))
                                    ) : (
                                        <div className="report-empty">
                                            <CheckCheck size={30}/>
                                            <p>{t("No warnings")}</p>
                                        </div>
                                    )}
                                    {sceneError && <p>{sceneError}</p>}
                                    <button
                                        className="button subtle"
                                        onClick={() => closeAfter(run({type: "recover"}))}
                                    >
                                        <History size={15}/>
                                        {t("Recover")}
                                    </button>
                                </div>
                            </>
                        )}
                        {modal === "prefab" && <>
                            {modalHeader("Create prefab", "Prefab selection hint")}
                            <div className="modal-body">
                                <label className="field"><span>{t("Name")}</span><input autoFocus value={prefabName}
                                                                                        maxLength={80}
                                                                                        onChange={e => setPrefabName(e.target.value)}
                                                                                        onKeyDown={e => {
                                                                                            if (e.key === "Enter" && canSavePrefab && prefabName.trim() && !busy) {
                                                                                                void closeAfter(run({
                                                                                                    type: "save_prefab",
                                                                                                    level,
                                                                                                    ids: selected,
                                                                                                    name: prefabName
                                                                                                }, "Prefab saved"));
                                                                                            }
                                                                                        }}/></label>
                                <p className="muted tiny">{t("Objects")}: {selection.length} · {t("My prefabs")}</p>
                            </div>
                            <div className="modal-footer">
                                <button className="button subtle" onClick={() => setModal(null)}>{t("Cancel")}</button>
                                <button className="button primary"
                                        disabled={!canSavePrefab || !prefabName.trim() || !!busy}
                                        onClick={() => void closeAfter(run({
                                            type: "save_prefab",
                                            level,
                                            ids: selected,
                                            name: prefabName
                                        }, "Prefab saved"))}>
                                    <Save size={16}/>{t("Save")}</button>
                            </div>
                        </>}
                        {modal === "fabric" && (
                            <>
                                {modalHeader("Fabric", "Fabric hint")}
                                <div className="modal-body">
                                    <label className="field">
                                        <span>{t("Name")}</span>
                                        <input
                                            value={fabricName}
                                            onChange={(e) => setFabricName(e.target.value)}
                                        />
                                    </label>
                                    <label className="field">
                                        <span>{t("Author")}</span>
                                        <input
                                            value={fabricAuthor}
                                            onChange={(e) => setFabricAuthor(e.target.value)}
                                        />
                                    </label>
                                    <p className="muted tiny">
                                        {fabricIds.length} / {available.length} {t("items")}
                                    </p>
                                    <div className="fabric-list">
                                        {available.map((a) => (
                                            <label key={a.meta.id}>
                                                <input
                                                    type="checkbox"
                                                    checked={fabricIds.includes(a.meta.id)}
                                                    onChange={(e) =>
                                                        setFabricIds((ids) =>
                                                            e.target.checked
                                                                ? [...ids, a.meta.id]
                                                                : ids.filter((id) => id !== a.meta.id),
                                                        )
                                                    }
                                                />
                                                <span>{a.meta.name}</span>
                                                <small>{t(categories[a.category] ?? a.category)}</small>
                                            </label>
                                        ))}
                                    </div>
                                </div>
                                <div className="modal-footer">
                                    <button
                                        className="button subtle"
                                        onClick={() => setModal(null)}
                                    >
                                        {t("Cancel")}
                                    </button>
                                    <button
                                        className="button primary"
                                        disabled={!!busy || !fabricIds.length}
                                        onClick={() =>
                                            closeAfter(
                                                run({
                                                    type: "fabric",
                                                    name: fabricName,
                                                    author: fabricAuthor,
                                                    ids: fabricIds,
                                                }),
                                            )
                                        }
                                    >
                                        <Package size={16}/>
                                        {t("Save fabric")}
                                    </button>
                                </div>
                            </>
                        )}
                        {modal === "asset" && activeAsset && (
                            <>
                                {modalHeader("Edit asset")}
                                <div className="modal-body">
                                    <div className="variant-preview">
                                        <Thumbnail id={activeAsset.meta.id}/>
                                    </div>
                                    <span className="eyebrow">{t("Library metadata")}</span>
                                    {textField("Name", activeAsset.meta.name, (name) =>
                                        changeAsset({name}),
                                    )}
                                    {textField("Tags", activeAsset.meta.tags.join(", "), (v) =>
                                        changeAsset({
                                            tags: v
                                                .split(",")
                                                .map((t) => t.trim())
                                                .filter(Boolean),
                                        }),
                                    )}
                                    <div className="two-fields">
                                        <NumberField
                                            label={t("Asset size") + " X"}
                                            value={activeAsset.meta.size.x}
                                            min={0.01}
                                            max={512}
                                            step={0.1}
                                            onChange={(x) =>
                                                changeAsset({size: {...activeAsset.meta.size, x}})
                                            }
                                        />
                                        <NumberField
                                            label={t("Asset size") + " Y"}
                                            value={activeAsset.meta.size.y}
                                            min={0.01}
                                            max={512}
                                            step={0.1}
                                            onChange={(y) =>
                                                changeAsset({size: {...activeAsset.meta.size, y}})
                                            }
                                        />
                                    </div>
                                    <div className="two-fields">
                                        <NumberField
                                            label={t("Pivot") + " X"}
                                            value={activeAsset.meta.pivot.x}
                                            min={-10}
                                            max={10}
                                            step={0.05}
                                            onChange={(x) =>
                                                changeAsset({pivot: {...activeAsset.meta.pivot, x}})
                                            }
                                        />
                                        <NumberField
                                            label={t("Pivot") + " Y"}
                                            value={activeAsset.meta.pivot.y}
                                            min={-10}
                                            max={10}
                                            step={0.05}
                                            onChange={(y) =>
                                                changeAsset({pivot: {...activeAsset.meta.pivot, y}})
                                            }
                                        />
                                    </div>
                                    <label className="field">
                                        <span>{t("Color mode")}</span>
                                        <select
                                            value={activeAsset.meta.color_mode}
                                            onChange={(e) =>
                                                changeAsset({
                                                    color_mode: e.target
                                                        .value as Asset["meta"]["color_mode"],
                                                })
                                            }
                                        >
                                            <option value="none">{t("None")}</option>
                                            <option value="tint">{t("Tint")}</option>
                                            <option value="red_mask">{t("Red mask")}</option>
                                        </select>
                                    </label>
                                    <label className="field">
                                        <span>{t("Variant name")}</span>
                                        <input
                                            value={variantName}
                                            onChange={(e) => setVariantName(e.target.value)}
                                        />
                                    </label>
                                    <span className="eyebrow">{t("Crop")}</span>
                                    <div className="four-fields">
                                        {["X", "Y", "W", "H"].map((l, i) => (
                                            <NumberField
                                                key={l}
                                                label={l}
                                                value={crop[i]}
                                                min={i < 2 ? 0 : 1}
                                                max={65536}
                                                onChange={(v) =>
                                                    setCrop((c) => c.map((n, j) => (j === i ? v : n)))
                                                }
                                            />
                                        ))}
                                    </div>
                                    <Toggle
                                        label={t("Flip horizontal")}
                                        checked={flipX}
                                        onChange={setFlipX}
                                    />
                                    <Toggle
                                        label={t("Flip vertical")}
                                        checked={flipY}
                                        onChange={setFlipY}
                                    />
                                    <NumberField
                                        label={t("Rotation") + " × 90°"}
                                        value={turns}
                                        min={0}
                                        max={3}
                                        onChange={setTurns}
                                    />
                                </div>
                                <div className="modal-footer">
                                    <button
                                        className="button subtle"
                                        onClick={() => setModal(null)}
                                    >
                                        {t("Cancel")}
                                    </button>
                                    <button
                                        className="button primary"
                                        disabled={!!busy}
                                        onClick={() =>
                                            closeAfter(
                                                run({
                                                    type: "crop",
                                                    asset: activeAsset.meta.id,
                                                    name: variantName,
                                                    rect: crop,
                                                    flip_x: flipX,
                                                    flip_y: flipY,
                                                    turns,
                                                }),
                                            )
                                        }
                                    >
                                        <Scissors size={16}/>
                                        {t("Variant")}
                                    </button>
                                </div>
                            </>
                        )}
                    </section>
                </div>
            )}
        </div>
    );
}
