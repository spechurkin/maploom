import ts from "typescript";

/** Compile inside the isolated frame. The returned factory is loaded as a blob script,
 * never evaluated in the application window; neither eval nor unsafe-eval is needed. */
export function compileModule(id: string, source: string) {
    const result = ts.transpileModule(source, {
        fileName: id,
        reportDiagnostics: true,
        compilerOptions: {
            target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS,
            jsx: ts.JsxEmit.ReactJSX, esModuleInterop: true, sourceMap: false
        },
    });
    const errors = result.diagnostics?.filter(d => d.category === ts.DiagnosticCategory.Error) ?? [];
    if (errors.length) throw Error(`${id}: ${errors.map(d => ts.flattenDiagnosticMessageText(d.messageText, "\n")).join("\n")}`);
    return `globalThis.__maploomRegister(${JSON.stringify(id)}, function(require, module, exports) {\n${result.outputText}\n});`;
}
