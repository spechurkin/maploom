# Theme installer and updater verification

[Theme API](themes.md)

The preservation rule is structural: user files under `themes/` never appear in an installer resource/file list.
Themes are outside every file or bundle that an updater replaces. Directory creation does not install example files.

## Windows

The NSIS template in the installed CLI version (`2.12.2`) deletes explicitly listed files and calls a non-recursive
`RMDir` on the installation root. The [project hook](../resources/installers/themes.nsh) creates `themes` on
installation
and ensures it remains present at uninstall. The optional “delete app data” action targets bundle-ID directories in
AppData, not `themes` in the install directory.

The [MSI fragment](../resources/installers/themes.wxs) creates a permanent directory component and contains no `File`
entries for themes. The standard MSI template's `RemoveFolder` removes empty directories, not user files recursively.
The fragment supports both x86 and x64 component bitness.

Run [check-theme-windows.ps1](../scripts/check-theme-windows.ps1) against a package output directory. It refuses to
replace a registered Maploom installation. NSIS tests use a fresh repository artifact directory and suppress shortcuts.
They exercise installation, same-version installation over existing files, `/UPDATE`, and uninstall, comparing SHA-256
of Unicode-named theme files. Full MSI installation/repair/uninstall runs only on an isolated CI runner; the script
accepts `-NsisOnly` for safe local NSIS checks.

## Linux

Tauri packages the executable under `/usr/bin` and resources under `/usr/lib/Maploom`. The loader therefore uses
`/usr/lib/Maploom/themes` for DEB/RPM. The [post-install hook](../resources/installers/themes-postinstall.sh) creates
this
directory without changing existing content or permissions. It is not part of either payload, and no pre/post-remove
hook deletes it. This preserves content on DEB remove/purge and RPM erase; it does not grant users write access to a
system directory.

[check-theme-packages.mjs](../scripts/check-theme-packages.mjs) installs actual DEB/RPM artifacts on an ephemeral native
Linux CI runner, writes Unicode-named theme files, reinstalls the package, removes it, and checks file hashes. DEB also
checks reinstall after remove and purge. Dependency checks are disabled only for these package lifecycle checks; the
editor is not launched by them. The script rejects local system installation and an already installed package.

AppImage themes are beside `$APPIMAGE`, outside its temporary read-only mount. The updater replaces the executable
file, so a sibling folder survives. The test copies/replaces/removes the real AppImage artifact and checks sibling file
hashes; it does not execute an interactive updater or verify an update download/signature handshake.

## macOS

A DMG is a read-only distribution image, not an uninstaller. Copy `Maploom.app` to its final writable containing folder;
`themes` is a sibling. Both Finder replacement/removal and Tauri's native updater replace/remove the whole `.app`.
Putting user themes inside `.app/Contents` would lose them and modify the signed bundle, so that location is rejected
by the directory policy. Disk-image launch/App Translocation can produce temporary read-only paths; finish installing
the application before adding themes.

The native macOS test mounts the real DMG read-only, copies its `.app` into a fresh artifact directory, creates sibling
themes, copies/replaces/removes only `.app`, verifies the copied signature, and compares theme hashes. This verifies
the actual archive and native filesystem lifecycle; it does not run Finder or the GUI updater.

## Evidence levels

The [release workflow](../.github/workflows/build.yml) runs the native checks for all six platform/architecture builds
before uploading artifacts. Adding these checks is not itself evidence that a remote workflow has run. Check the
specific workflow results for the produced release. Windows development runs cannot establish native macOS/Linux
installer execution; source review and path/payload tests are separate evidence.

The reviewed primary sources are
the [NSIS template](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.2/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi),
[MSI template](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.2/crates/tauri-bundler/src/bundle/windows/msi/main.wxs),
[DEB packager](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.2/crates/tauri-bundler/src/bundle/linux/debian.rs),
[RPM packager](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.2/crates/tauri-bundler/src/bundle/linux/rpm.rs),
[macOS bundler](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.2/crates/tauri-bundler/src/bundle/macos/app.rs),
and the locally locked `tauri-plugin-updater 2.10.1` source for executable and `.app` replacement.

Manual recursive deletion of the containing directory or third-party “clean uninstall” tools is outside this guarantee.
Installer tests cover same-version replacement/repair and removal; cross-version upgrade, interrupted update recovery,
and changing the installation location are distinct scenarios and must not be reported as tested by these checks.
