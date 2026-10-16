# Localizing Maploom

[README](../README.md)

Interface translations are stored in JSON files. To correct a label or install an additional language, edit its language
catalog and restart Maploom. English and Russian are the built-in languages.

Adding a language only requires a new JSON catalog in `locales/`. The existing catalog loading and packaging code does
not need changes. Source files containing text outside the catalogs are listed
under [Text handled separately](#text-handled-separately).

## Catalog structure

A minimal example for an additional language:

```json
{
  "format": "maploom/locale",
  "version": 1,
  "id": "de",
  "name": "Deutsch",
  "strings": {
    "Open": "Öffnen",
    "Save": "Speichern",
    "Settings": "Einstellungen"
  }
}
```

Keep `format` and `version` unchanged. `id` uniquely identifies the language; `name` is its display name in Settings.
Prefer the language's own name, such as `Deutsch` or `Français`.

Keys inside `strings` are stable, case-sensitive identifiers: `Assets` and `assets` are different keys. Some look like
ordinary English phrases (`"Save"`); others describe a message's purpose (`"Export description"`). Translate only the
values on the right. To change the save button's label, change the value of `"Save"` while preserving the key.

### JSON requirements

Catalogs must meet these requirements:

- UTF-8 encoding; an initial UTF-8 BOM is accepted.
- A file size of at most 1 MiB.
- `format` equal to `maploom/locale` and `version` equal to `1`.
- An `id` of 1–32 ASCII characters: letters, digits, `-`, or `_`.
- A `name` that is not empty or whitespace-only.
- At most 10,000 entries in `strings`; nonempty keys up to 8192 bytes and values up to 16,384 bytes.

Both keys and values must be strings. JSON comments and trailing commas are not allowed. Do not duplicate keys. Escape
quotation marks inside a value as `\"` and line breaks as `\n`.

## Add a language

1. Copy [locales/en.json](../locales/en.json) to `locales/de.json`, or a filename matching your chosen ID.
2. Set a new `id` and a clear `name`.
3. Translate the values in `strings`. Preserve keys, technical format names, and keyboard notation.
4. Place the JSON in a `locales` folder accessible to the app, for example beside `Maploom.exe` in a portable build.
5. Fully close and restart Maploom. Catalogs are loaded at startup; editing JSON while the app is running does not
   automatically reload it.
6. Open **Settings → Language** and select the added language. Its entry uses `id` and `name` from the file.

During development, keep the new file in the project's [locales/](../locales/) folder and run `npm run desktop` from the
project root. The next packaging run includes this folder; Windows portable packaging also copies the entire folder.

This is an external catalog. It becomes available in the desktop app, but is not automatically embedded in the browser
preview or in a binary running without external resources.

## Missing translations

An incomplete additional language usually displays English for untranslated entries. Copy the complete English catalog
and translate every value to avoid gaps. Keep values nonempty.

The selected language is saved between sessions. If its catalog is unavailable at the next startup, the desktop app
returns to Russian.

## Troubleshooting

- **The language does not appear:** check the actual `locales` folder, JSON extension, required fields, and Activity,
  then restart.
- **Another translation loads:** check duplicate `id` values across files and the launch working directory. Later
  catalogs replace earlier ones.
- **Some text remains English:** find the key in the source; it may be missing from your JSON or belong to native
  diagnostics.
- **The desktop translation works but the browser does not:** Rust loads external catalogs. The preview needs a built-in
  import or separate snapshot data.
- **The app returns to Russian after restarting:** the saved language ID was not found at startup; check that its
  external file is still accessible.
