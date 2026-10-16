import {useEffect, useRef, useState} from "react";
import {ImageOff, Package} from "lucide-react";
import {thumbnail} from "../app/bridge";
import {ThumbnailQueue} from "./thumbnailQueue";

const queue = new ThumbnailQueue();
let epoch = 0;

export function resetThumbnails() {
    queue.reset();
    epoch++;
}

export function Thumbnail({id, color, maxSide, frameClassName}: {
    id: string; color?: number[]; maxSide?: number; frameClassName?: string;
}) {
    const frame = useRef<HTMLSpanElement>(null);
    const [src, setSrc] = useState(""),
        [failed, setFailed] = useState(false);
    const key = JSON.stringify([color, maxSide, epoch]);
    useEffect(() => {
        let alive = true;
        let request: ReturnType<ThumbnailQueue["request"]> | undefined;
        setSrc("");
        setFailed(false);
        const start = () => {
            if (request) return;
            request = queue.request(JSON.stringify([id, color, maxSide, epoch]),
                () => thumbnail(id, color, maxSide));
            request.promise.then(
                (s) => {
                    if (alive) setSrc(s);
                },
                () => {
                    if (alive) setFailed(true);
                },
            );
        };
        // Gallery pages can contain many expensive textures below the scroll area.
        // Load visible cards first; the material controls remain eager.
        let observer: IntersectionObserver | undefined;
        if (frame.current && typeof IntersectionObserver !== "undefined") {
            observer = new IntersectionObserver(entries => {
                if (entries.some(entry => entry.isIntersecting)) {
                    start();
                    observer?.disconnect();
                }
            }, {rootMargin: "120px"});
            observer.observe(frame.current);
        } else start();
        return () => {
            alive = false;
            observer?.disconnect();
            request?.release();
        };
    }, [id, key, frameClassName]);
    const content = src ? (
        <img src={src} alt="" draggable={false} onError={() => {
            setSrc("");
            setFailed(true);
        }}/>
    ) : failed ? (
        <ImageOff size={20}/>
    ) : (
        <Package size={22} className="placeholder-icon"/>
    );
    return frameClassName ? <span ref={frame} className={frameClassName}>{content}</span> : content;
}
