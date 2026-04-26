import { readFileSync, writeFileSync } from "node:fs";

const path = "build/worker/shim.mjs";
let src = readFileSync(path, "utf8");

if (src.includes("WebAssembly.instantiate(")) {
  console.log("[patch-shim] already patched");
  process.exit(0);
}

const importRe =
  /import (\w+) from"\.\/index\.wasm";import\{WorkerEntrypoint as (\w+)\}from"cloudflare:workers";\(void 0\)\?\.\(\);/;
const m = src.match(importRe);
if (!m) {
  console.error("[patch-shim] init marker not found — shim format changed");
  process.exit(1);
}
const [, wasmVar, entryVar] = m;

const setWasmRe = /function (\w+)\(t\)\{c=t\}/;
const sm = src.match(setWasmRe);
if (!sm) {
  console.error("[patch-shim] could not locate __wbg_set_wasm function");
  process.exit(1);
}
const [, setWasmFn] = sm;

const replacement =
  `import ${wasmVar} from"./index.wasm";` +
  `import{WorkerEntrypoint as ${entryVar}}from"cloudflare:workers";` +
  `const __wasmImports=new Proxy({},{get:()=>new Proxy({},{get(_,k){return p[k]}})});` +
  `const __wasmInstance=await WebAssembly.instantiate(${wasmVar},__wasmImports);` +
  `${setWasmFn}(__wasmInstance.exports);` +
  `__wasmInstance.exports.__wbindgen_start?.();`;

src = src.replace(importRe, replacement);

src = src.replace(
  /async queue\(e\)\{return await\(void 0\)\(e,this\.env,this\.ctx\)\}/,
  "",
);
src = src.replace(
  /async scheduled\(e\)\{return await\(void 0\)\(e,this\.env,this\.ctx\)\}/,
  "",
);

writeFileSync(path, src);
console.log(`[patch-shim] patched (wasm=${wasmVar}, setWasm=${setWasmFn})`);
