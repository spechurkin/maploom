import {createRoot} from "react-dom/client";

// Only the public appearance capabilities used by this portable template.
type Appearance = Readonly<{
    mode: "dark" | "light";
    accent: readonly number[];
    locale: string;
    screen: "welcome" | "editor";
}>;
type ThemeAPI = Readonly<{
    root: HTMLElement;
    appearance: Appearance;
    setTokens(tokens: Record<string, string>): void;
    onAppearanceChange(listener: (appearance: Appearance) => void): () => void;
}>;

function Decoration({appearance, copper}: { appearance: Appearance; copper: string }) {
    if (appearance.screen !== "welcome") return null;
    const ru = appearance.locale.toLowerCase().startsWith("ru");
    const directions = ru ? ["С", "В", "Ю", "З"] : ["N", "E", "S", "W"];
    return <div className="atlas-mark" aria-hidden="true" style={{color: copper}}>
        <style>{`
            .atlas-mark {position:absolute;right:38px;bottom:30px;width:196px;pointer-events:none;
                opacity:.32;font-family:Georgia,'Times New Roman',serif;text-align:center;
                animation:atlas-arrive 900ms ease-out both}
            .atlas-mark svg {display:block;width:100%;height:auto;overflow:visible}
            .atlas-mark .atlas-orbit {transform-origin:120px 120px;animation:atlas-orbit 100s linear infinite}
            .atlas-mark .atlas-title {margin-top:7px;font-size:13px;letter-spacing:.28em;text-transform:uppercase}
            .atlas-mark .atlas-caption {margin-top:7px;font:10px system-ui,sans-serif;letter-spacing:.08em}
            @keyframes atlas-arrive {from {opacity:0;transform:translateY(8px)} to {opacity:.32;transform:none}}
            @keyframes atlas-orbit {to {transform:rotate(360deg)}}
            @media (max-width:1100px), (max-height:760px) {.atlas-mark {width:134px;right:18px;bottom:18px;opacity:.2;animation:none}}
            @media (max-width:760px) {.atlas-mark {display:none}}
            @media (prefers-reduced-motion:reduce) {.atlas-mark,.atlas-mark .atlas-orbit {animation:none}}
        `}</style>
        <svg viewBox="0 0 240 240" fill="none" stroke="currentColor" strokeWidth=".8">
            <circle cx="120" cy="120" r="96"/>
            <circle cx="120" cy="120" r="89" strokeDasharray="1 5"/>
            <g className="atlas-orbit">
                <circle cx="120" cy="120" r="104" strokeDasharray="74 90"/>
                <circle cx="224" cy="120" r="2" fill="currentColor"/>
            </g>
            {Array.from({length: 48}, (_, index) => <path key={index}
                                                          transform={`rotate(${index * 7.5} 120 120)`}
                                                          d={`M120 24V${index % 4 === 0 ? 35 : 29}`}/>)}
            <path d="M120 46 136 104 194 120 136 136 120 194 104 136 46 120 104 104Z"/>
            <path
                d="M120 46 120 120 104 104Z M194 120 120 120 136 104Z M120 194 120 120 136 136Z M46 120 120 120 104 136Z"
                fill="currentColor" fillOpacity=".45"/>
            <path d="M69 69 106 112 M171 69 134 112 M171 171 134 128 M69 171 106 128"/>
            <circle cx="120" cy="120" r="9" fill="currentColor" fillOpacity=".2"/>
            {directions.map((label, index) => {
                const [x, y] = [[120, 15], [230, 124], [120, 235], [10, 124]][index];
                return <text key={index} x={x} y={y} textAnchor="middle" stroke="none" fill="currentColor"
                             fontSize="11" fontFamily="Georgia,serif">{label}</text>;
            })}
        </svg>
        <div className="atlas-title">{ru ? "Атлас" : "Atlas"}</div>
        <div className="atlas-caption">{ru ? "У каждой истории — свой горизонт" : "Every story has a horizon"}</div>
    </div>;
}

export default function activate(api: ThemeAPI) {
    const root = createRoot(api.root);

    function paint(appearance: Appearance) {
        const dark = appearance.mode === "dark";
        const base = dark ? [211, 161, 107] : [128, 79, 45];
        const panel = dark ? [26, 42, 46] : [255, 250, 241];
        // Keep the copper character, while honoring the user's accent choice.
        const accent = base.map((value, index) => Math.round(value * .75 + (appearance.accent[index] ?? value) * .25));
        const rgb = (value: number[]) => `rgb(${value.join(",")})`;
        const linear = accent.map(value => {
            const channel = value / 255;
            return channel <= .04045 ? channel / 12.92 : ((channel + .055) / 1.055) ** 2.4;
        });
        const luminance = linear[0] * .2126 + linear[1] * .7152 + linear[2] * .0722;
        api.setTokens({
            "--accent": rgb(accent),
            "--accent-ink": luminance > .179 ? "#000000" : "#ffffff",
            "--active": rgb(panel.map((value, index) => Math.round(value * .84 + accent[index] * .16))),
        });
        root.render(<Decoration appearance={appearance} copper={rgb(accent)}/>);
    }

    paint(api.appearance);
    const unsubscribe = api.onAppearanceChange(paint);
    return () => {
        unsubscribe();
        root.unmount();
    };
}
