import {test} from "node:test";
import assert from "node:assert/strict";
import {
    type AssetCatalog,
    catalogPackEnabled,
    catalogSelectionState,
    displayDirectoryPath,
    duplicatePackFiles,
    type Project,
    setCatalogPack,
    setCatalogEnabled
} from "../src/map/model";

function catalog(id = "root"): AssetCatalog {
    return {
        directory: {id, name: "Packs", path: "S:\\Dungeondraft Packs"},
        assets: 3, loose_images: 1, warnings: [],
        packs: ["a", "b"].map(key => ({
            key: `${key}.mloom_fabric`,
            path: `S:\\Dungeondraft Packs\\${id}\\${key}.mloom_fabric`,
            name: key,
            file: `${key}.mloom_fabric`,
            assets: 1,
            metadata: {id: key, author: "", version: "", description: "", website: "", logo: null},
        })),
    };
}

const project = () => ({asset_catalogs: []} as unknown as Project);

test("whole catalog includes loose images and future packs, survives serialization, and can be disabled", () => {
    const p = project(), c = catalog();
    assert.equal(catalogSelectionState(p, c), "none");
    setCatalogPack(p, [c], c, c.packs[0].key, true);
    assert.equal(catalogSelectionState(p, c), "partial");
    setCatalogEnabled(p, [c], c, true);
    const restored: Project = JSON.parse(JSON.stringify(p));
    c.packs.push({...c.packs[0], key: "new.pack"});
    assert.equal(catalogSelectionState(restored, c), "all");
    for (const key of [...c.packs.map(pack => pack.key), ""]) {
        assert.equal(catalogPackEnabled(restored, c, key), true);
    }
    assert.equal(JSON.stringify(restored).includes(c.directory.path), false);
    setCatalogPack(restored, [c], c, c.packs[1].key, false);
    assert.equal(catalogSelectionState(restored, c), "partial");
    assert.equal(catalogPackEnabled(restored, c, "new.pack"), true);
    setCatalogEnabled(restored, [c], c, false);
    assert.deepEqual(restored.asset_catalogs, []);
});

test("whole catalog synchronizes duplicate IDs and preserves other selections and precedence", () => {
    const p = project(), a = catalog(), b = catalog("other");
    a.packs[1].metadata.id = b.packs[1].metadata.id = "";
    b.packs.push({...b.packs[0], key: "nested/copy.pack"});
    p.asset_catalogs = [{id: "missing", name: "Missing", packs: ["lost.pack"]}];
    setCatalogEnabled(p, [a, b], a, true);
    assert.deepEqual(p.asset_catalogs.map(ref => ref.id), ["missing", "root", "other"]);
    assert.deepEqual(p.asset_catalogs[2].packs, [b.packs[0].key, "nested/copy.pack"]);
    assert.equal(catalogPackEnabled(p, b, b.packs[1].key), false);
    assert.equal(catalogPackEnabled(p, b, ""), false);
    setCatalogEnabled(p, [a, b], b, true);
    setCatalogEnabled(p, [a, b], a, false);
    assert.deepEqual(p.asset_catalogs, [
        {id: "missing", name: "Missing", packs: ["lost.pack"]},
        {id: b.directory.id, name: b.directory.name, packs: [b.packs[1].key, ""]},
    ]);
});

test("catalog checkbox recognizes manual select-all, legacy selections and empty catalogs", () => {
    const p = project(), c = catalog();
    for (const key of [...c.packs.map(pack => pack.key), ""]) setCatalogPack(p, [c], c, key, true);
    assert.equal(catalogSelectionState(p, c), "all");
    p.asset_catalogs = [{id: c.directory.id, name: c.directory.name}];
    assert.equal(catalogSelectionState(p, c), "all");
    c.packs = [];
    c.loose_images = 0;
    setCatalogEnabled(p, [c], c, false);
    assert.equal(catalogSelectionState(p, c), "none");
    setCatalogEnabled(p, [c], c, true);
    assert.equal(catalogSelectionState(p, c), "all");
});

test("packs and loose images toggle independently and never persist the directory path", () => {
    const p = project(), c = catalog();
    setCatalogPack(p, [c], c, c.packs[0].key, true);
    assert.equal(catalogPackEnabled(p, c, c.packs[0].key), true);
    assert.equal(catalogPackEnabled(p, c, c.packs[1].key), false);
    assert.equal(catalogPackEnabled(p, c, ""), false);
    assert.equal(JSON.stringify(p).includes(c.directory.path), false);
    assert.deepEqual(Object.keys(p.asset_catalogs[0]).sort(), ["id", "name", "packs"]);
    setCatalogPack(p, [c], c, "", true);
    setCatalogPack(p, [c], c, c.packs[0].key, false);
    assert.deepEqual(p.asset_catalogs[0].packs, [""]);
    setCatalogPack(p, [c], c, "", false);
    assert.deepEqual(p.asset_catalogs, []);
});

test("legacy whole-directory selections keep the other packs when one is disabled", () => {
    const p = project(), c = catalog();
    p.asset_catalogs = [{id: c.directory.id, name: c.directory.name}];
    assert.equal(catalogPackEnabled(p, c, c.packs[0].key), true);
    setCatalogPack(p, [c], c, c.packs[0].key, false);
    assert.deepEqual(p.asset_catalogs[0].packs, [c.packs[1].key, ""]);
});

test("matching pack IDs toggle together within and across directories, including hidden copies", () => {
    const p = project(), a = catalog(), b = catalog("other");
    a.packs.push({...a.packs[0], key: "nested/copy.mloom_fabric"});
    b.packs[1].metadata.id = "unrelated";
    p.asset_catalogs = [{id: "missing", name: "Missing", packs: ["lost.pack"]}];
    setCatalogPack(p, [a, b], a, a.packs[0].key, true);
    for (const [c, key] of [[a, a.packs[0].key], [a, a.packs[2].key], [b, b.packs[0].key]] as const) {
        assert.equal(catalogPackEnabled(p, c, key), true);
    }
    assert.equal(catalogPackEnabled(p, a, a.packs[1].key), false);
    setCatalogPack(p, [a, b], b, b.packs[0].key, false);
    assert.deepEqual(p.asset_catalogs, [{id: "missing", name: "Missing", packs: ["lost.pack"]}]);
});

test("empty IDs are independent and toggles preserve catalog precedence", () => {
    const p = project(), a = catalog(), b = catalog("other");
    a.packs[0].metadata.id = b.packs[0].metadata.id = "";
    setCatalogPack(p, [a, b], a, a.packs[0].key, true);
    setCatalogPack(p, [a, b], b, b.packs[1].key, true);
    setCatalogPack(p, [a, b], a, a.packs[1].key, true);
    assert.equal(catalogPackEnabled(p, b, b.packs[0].key), false);
    assert.deepEqual(p.asset_catalogs.map(ref => ref.id), [a.directory.id, b.directory.id]);
});

test("display paths remove Windows extended drive and UNC prefixes only", () => {
    assert.equal(displayDirectoryPath("\\\\?\\S:\\Dungeondraft Packs"), "S:\\Dungeondraft Packs");
    assert.equal(displayDirectoryPath("\\\\?\\UNC\\server\\packs"), "\\\\server\\packs");
    for (const path of ["S:\\Packs", "\\\\server\\packs", "/packs", "\\\\?\\Volume{test}\\packs"]) {
        assert.equal(displayDirectoryPath(path), path);
    }
});

test("duplicate hints group full file paths by nonempty ID without repeating directory aliases", () => {
    const a = catalog(), b = catalog("other");
    a.packs[0].path = "\\\\?\\S:\\Packs\\first.pack";
    b.packs[0].path = "S:\\Packs\\second.pack";
    a.packs[1].metadata.id = b.packs[1].metadata.id = "";
    assert.deepEqual(duplicatePackFiles([a, b, a]), [["S:\\Packs\\first.pack", "S:\\Packs\\second.pack"]]);
    assert.deepEqual(duplicatePackFiles([a]), []);
});
