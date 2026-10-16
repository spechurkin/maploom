// Only this fixed bootstrap is permitted inline by its CSP hash.
addEventListener("message", function bootstrap(event) {
    if (event.source !== parent || event.data?.type !== "maploom:theme:bootstrap" || !event.ports[0]) return;
    removeEventListener("message", bootstrap);
    const url = URL.createObjectURL(new Blob([event.data.source], {type: "text/javascript"}));
    const script = document.createElement("script");
    script.src = url;
    script.onload = () => {
        URL.revokeObjectURL(url);
        dispatchEvent(new MessageEvent("message", {source: parent, data: event.data.init, ports: event.ports}));
    };
    script.onerror = () => {
        URL.revokeObjectURL(url);
        event.ports[0].postMessage({type: "error", error: "Cannot start the theme sandbox"});
    };
    document.head.append(script);
});
