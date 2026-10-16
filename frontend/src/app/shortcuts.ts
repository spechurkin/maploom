// Physical letter keys keep editor commands working in every keyboard layout.
export function shortcutKey(event: Pick<KeyboardEvent, "code" | "key">): string {
    return /^Key[A-Z]$/.test(event.code) ? event.code.slice(3).toLowerCase() : event.key.toLowerCase();
}
