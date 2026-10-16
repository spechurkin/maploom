# Theme files

[README](../README.md)

Open **Settings → Appearance → Open themes folder**. Add a folder with any name, or use **Create example themes**.
Edit the files below, then click **Reload themes**.

The example button copies three CSS-only templates, **Monokai** (dark), **GitHub** (light) and **Contour** (light),
and a CSS + TSX package, **Atlas**, with two variants: **Atlas Dark** and **Atlas Light**.
Monokai and GitHub use palettes adapted from the classic JetBrains color schemes. Each theme keeps its own light or dark
colors, including its card preview. Templates are created only on request; existing files are never overwritten.
Repeated creation uses new folders such as `Monokai 2` and `GitHub 2`.

```text
themes/
  My Theme [夜]/
    theme.json       Required: theme name and file paths.
    style.css        Colors, fonts and interface styling.
    theme.tsx        Optional: dynamic styling or decorations; JS/JSX/TS also work.
    images/         Optional: images referenced by your CSS or code.
    fonts/          Optional: font files referenced by your CSS.
```

## `theme.json`

For a CSS-only theme:

```json
{
  "format": "maploom/theme",
  "api_version": 1,
  "name": "My Theme [夜]",
  "mode": "dark",
  "stylesheet": "style.css"
}
```

Change `name` and the file paths. Keep `format` and `api_version` unchanged.
Add `"entry": "theme.tsx"` if the theme uses code. At least one of `stylesheet` or `entry` is required. Set `mode` to `"dark"` or `"light"` to declare the theme's type.

### Multiple variants in one folder

Use `variants` to offer multiple cards that share the package's CSS, TSX, images and fonts:

```json
{
  "format": "maploom/theme",
  "api_version": 1,
  "name": "Atlas",
  "stylesheet": "style.css",
  "entry": "theme.tsx",
  "variants": [
    { "id": "dark", "name": "Atlas Dark", "mode": "dark" },
    { "id": "light", "name": "Atlas Light", "mode": "light" }
  ]
}
```

Each variant has a unique, stable `id`, a display `name` and a required `mode` (`dark` or `light`).
Maploom shows one card per variant and remembers the folder and variant separately.
Selecting either card sets the logo, base palette, `data-theme-mode`, preview and `api.appearance.mode` to that variant's type.
CSS can switch palettes using `[data-theme-mode="light"]`; TSX can use `api.appearance.mode`.
The first variant is the default when an older saved selection has no variant. An unavailable variant falls back
to the built-in appearance and is marked unavailable. Themes without `variants` retain their single card.

## `style.css`

Edit the theme's colors and styling. Start with the [Monokai stylesheet](../resources/theme-template/monokai/style.css)
or [GitHub stylesheet](../resources/theme-template/github/style.css).

Use `[data-theme-part="app"]` for the root and `[data-theme-mode="dark"]` or `[data-theme-mode="light"]` for
mode-specific rules.
Available parts: `app`, `header`, `workspace`, `tools`, `canvas`, `library`, `welcome`, `dialog`, `decoration`.

Available variables: `--accent`, `--accent-ink`, `--bg`, `--panel`, `--panel-alt`, `--input`, `--text`, `--muted`,
`--faint`, `--line`, `--hover`, `--canvas`, `--active`, `--shadow`, `--texture`, `--theme-font`.
Put the default palette here so it also appears in the theme card preview.

## `theme.tsx` or `theme.js` — optional

Add this file only for dynamic styling or decorations. Export a default `activate(api)` function.
Set `entry` in `theme.json` to the actual filename. Additional local modules are needed only if your code imports them.

## Images and fonts — optional

Add only the files your theme uses. Reference them with relative paths, such as `url("./images/moon.svg")`
or `url("./fonts/my-font.woff2")` in CSS, or `api.asset("images/moon.svg")` in code.
