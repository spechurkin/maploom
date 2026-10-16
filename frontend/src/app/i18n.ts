import ru from "../../../locales/ru.json";
import en from "../../../locales/en.json";

import type {Locale} from "../map/model";

export function translator(locale: string, catalogs?: Locale[]) {
    return (key: string) => {
        const custom = catalogs?.find((c) => c.id === locale)?.strings[key];
        if (custom?.trim()) return custom;
        return (
            (locale === "ru" ? ru : en).strings[key as keyof typeof ru.strings] ??
            en.strings[key as keyof typeof en.strings] ??
            key
        );
    };
}
