import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import ts from "typescript";

const repoRoot = path.resolve(import.meta.dirname, "..");
const outRoot = await mkdtemp(path.join(tmpdir(), "vision-desktop-state-test-"));

const files = [
  ["src/state/desktopReducer.ts", "src/state/desktopReducer.js"],
  ["src/state/desktopRequestTracker.ts", "src/state/desktopRequestTracker.js"],
  ["src/state/__tests__/desktopReducer.test.ts", "src/state/__tests__/desktopReducer.test.js"],
  ["src/state/__tests__/desktopRequestTracker.test.ts", "src/state/__tests__/desktopRequestTracker.test.js"],
  ["src/features/configuration/configurationStatus.ts", "src/features/configuration/configurationStatus.js"],
  ["src/features/node-manager/lifecycleControls.ts", "src/features/node-manager/lifecycleControls.js"],
  ["src/features/node-manager/lifecycleControls.test.ts", "src/features/node-manager/lifecycleControls.test.js"],
  ["src/features/configuration/configurationStatus.test.ts", "src/features/configuration/configurationStatus.test.js"],
  ["src/features/diagnostics/diagnosticsStatus.ts", "src/features/diagnostics/diagnosticsStatus.js"],
  ["src/features/diagnostics/diagnosticsStatus.test.ts", "src/features/diagnostics/diagnosticsStatus.test.js"],
  ["src/features/mining/miningStatus.ts", "src/features/mining/miningStatus.js"],
  ["src/features/mining/miningStatus.test.ts", "src/features/mining/miningStatus.test.js"],
  ["src/features/wallet/walletStatus.ts", "src/features/wallet/walletStatus.js"],
  ["src/features/wallet/walletStatus.test.ts", "src/features/wallet/walletStatus.test.js"],
];

try {
  for (const [sourceRelative, outputRelative] of files) {
    const sourcePath = path.join(repoRoot, sourceRelative);
    const outputPath = path.join(outRoot, outputRelative);
    const source = await readFile(sourcePath, "utf8");
    const transpiled = ts.transpileModule(source, {
      compilerOptions: {
        module: ts.ModuleKind.CommonJS,
        target: ts.ScriptTarget.ES2022,
        esModuleInterop: true,
        importsNotUsedAsValues: ts.ImportsNotUsedAsValues.Remove,
      },
      fileName: sourcePath,
    });
    await mkdir(path.dirname(outputPath), { recursive: true });
    await writeFile(outputPath, transpiled.outputText, "utf8");
  }

  await import(pathToFileURL(path.join(outRoot, "src/state/__tests__/desktopReducer.test.js")));
  await import(pathToFileURL(path.join(outRoot, "src/state/__tests__/desktopRequestTracker.test.js")));
  await import(pathToFileURL(path.join(outRoot, "src/features/configuration/configurationStatus.test.js")));
  await import(pathToFileURL(path.join(outRoot, "src/features/node-manager/lifecycleControls.test.js")));
  await import(pathToFileURL(path.join(outRoot, "src/features/diagnostics/diagnosticsStatus.test.js")));
  await import(pathToFileURL(path.join(outRoot, "src/features/mining/miningStatus.test.js")));
  await import(pathToFileURL(path.join(outRoot, "src/features/wallet/walletStatus.test.js")));
  console.log("Desktop state transition tests passed");
} finally {
  await rm(outRoot, { recursive: true, force: true });
}