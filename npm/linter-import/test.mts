import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { appendFile, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const [executable] = parseArgs({ allowPositionals: true }).positionals;
assert(executable, "Pass the path to a Sprawl Guard executable or npm wrapper");
const target = resolve(executable);
// Windows cannot start a JavaScript file directly, so the npm wrapper runs through Node.js.
const sprawlGuard: readonly [string, ...string[]] = target.endsWith(".js")
  ? [process.execPath, target]
  : [target];
const dependencies = fileURLToPath(new URL("./node_modules/", import.meta.url));
const source = "var a = 1;\n\nvar b = 2;\nvar c = 3;\n";
const env = { ...process.env };
delete env.ESLINT_USE_FLAT_CONFIG;
delete env.SPRAWL_GUARD_BINARY;

/** One linter whose limits and suppressions Sprawl Guard imports. */
interface LinterCase {
  readonly name: string;
  readonly linter: "eslint" | "oxlint";
  /** The package in this directory's `node_modules` that provides the linter. */
  readonly dependency: string;
  readonly config: string;
  readonly configText: string;
  /** Arguments that make the linter record bulk suppressions, if it supports them. */
  readonly bulkArgs?: readonly string[];
  /** Whether Oxlint reads `eslint-disable` directives as its own. */
  readonly importsEslintAlias?: boolean;
}

const cases: readonly LinterCase[] = [
  {
    name: "ESLint 8",
    linter: "eslint",
    dependency: "eslint-legacy",
    config: ".eslintrc.cjs",
    configText:
      'module.exports = { root: true, rules: { "max-lines": ["error", { max: 2, skipBlankLines: true }] }, overrides: [{ files: ["alias.js"], rules: { "max-lines": "off" } }] };',
  },
  {
    name: "ESLint 10",
    linter: "eslint",
    dependency: "eslint",
    config: "eslint.config.mjs",
    configText:
      'export default [{ rules: { "max-lines": ["error", { max: 2, skipBlankLines: true }] } }, { files: ["alias.js"], rules: { "max-lines": "off" } }];',
    bulkArgs: ["--suppress-rule", "max-lines"],
  },
  {
    name: "Oxlint",
    linter: "oxlint",
    dependency: "oxlint",
    config: ".oxlintrc.json",
    configText: '{"rules":{"max-lines":["error",{"max":2,"skipBlankLines":true}]}}',
    bulkArgs: ["--suppress-all"],
    importsEslintAlias: true,
  },
  {
    name: "Oxlint with ESLint directives disabled",
    linter: "oxlint",
    dependency: "oxlint",
    config: ".oxlintrc.json",
    configText:
      '{"options":{"respectEslintDisableDirectives":false},"rules":{"max-lines":["error",{"max":2,"skipBlankLines":true}]}}',
    bulkArgs: ["--suppress-all"],
    importsEslintAlias: false,
  },
];

for (const fixture of cases) {
  test(`${fixture.name} imports limits and baselines from an installed linter`, async (t) => {
    const root = await mkdtemp(join(tmpdir(), "sprawl guard linter "));
    t.after(() => rm(root, { recursive: true, force: true }));
    const modules = join(root, "node_modules");
    await mkdir(modules);
    // A junction also works on Windows runners without symlink privileges.
    const linterPackage = join(modules, fixture.linter);
    await symlink(join(dependencies, fixture.dependency), linterPackage, "junction");
    await writeFile(join(root, fixture.config), fixture.configText);
    await writeFile(join(root, "inline.js"), `/* ${fixture.linter}-disable max-lines */\n${source}`);
    if (fixture.linter === "oxlint") {
      await writeFile(join(root, "legacy.js"), `/* eslint-disable max-lines */\n${source}`);
    }
    await writeFile(join(root, "unsuppressed.js"), source);
    // This file is below the Sprawl Guard limit and must not be sent to ESLint.
    await writeFile(join(root, "unrelated-parse-error.js"), "function {\n");

    if (fixture.bulkArgs) {
      await writeFile(join(root, "bulk.js"), source);
      const entry = fixture.linter === "eslint" ? "eslint.js" : "oxlint";
      run(root, process.execPath, [join(linterPackage, "bin", entry), ...fixture.bulkArgs, "bulk.js"]);
    }

    const sample = fixture.linter === "eslint" ? ["inline.js"] : [];
    const configArgs = ["config", "import", fixture.linter, ...sample];
    const config = runSprawlGuard(root, configArgs);
    await assert.rejects(readFile(join(root, "sprawl-guard.toml")), { code: "ENOENT" });
    runSprawlGuard(root, ["config", "import", "--write", fixture.linter, ...sample]);
    assert.equal(await readFile(join(root, "sprawl-guard.toml"), "utf8"), config);

    // A followed link may resolve outside the project; the file keeps its in-project path.
    const outside = await mkdtemp(join(tmpdir(), "sprawl guard linked "));
    t.after(() => rm(outside, { recursive: true, force: true }));
    await writeFile(join(outside, "linked.js"), `/* ${fixture.linter}-disable max-lines */\n${source}`);
    const linked = await fileLink(join(outside, "linked.js"), join(root, "linked.js"));
    // Each path to one file keeps its own result; ESLint turns the rule off for this one.
    const aliased = await fileLink(join(root, "inline.js"), join(root, "alias.js"));
    await appendFile(join(root, "sprawl-guard.toml"), '\n[traversal]\nfollow_symlinks = "files"\n');

    const update = ["update", "--import", fixture.linter, "--format", "toml"];
    const preview = runSprawlGuard(root, update);
    const expectedPaths = fixture.bulkArgs ? ["bulk.js", "inline.js"] : ["inline.js"];
    if (fixture.importsEslintAlias) expectedPaths.push("legacy.js");
    if (linked) expectedPaths.push("linked.js");
    if (aliased && fixture.linter === "oxlint") expectedPaths.push("alias.js");
    assert.deepEqual(
      [...preview.matchAll(/^path = "(.*)"\r?$/gm)].map((match) => match[1]),
      expectedPaths.sort(),
    );
    await assert.rejects(readFile(join(root, "sprawl-guard.ratchet.toml")), { code: "ENOENT" });
    runSprawlGuard(root, [...update, "--write"]);
    const check = runSprawlGuard(root, ["check"], 1);
    assert.match(check, /unsuppressed\.js: 3 lines \(limit 2\)/);
    assert.doesNotMatch(check, /(?:bulk|inline|linked)\.js/);
    if (fixture.importsEslintAlias === false) assert.match(check, /legacy\.js: 4 lines \(limit 2\)/);
    if (aliased && fixture.linter === "eslint") assert.match(check, /alias\.js: 4 lines \(limit 2\)/);
  });
}

/** Links a file, and reports whether it could; Windows refuses without Developer Mode or administrator rights. */
function fileLink(target: string, path: string): Promise<boolean> {
  return symlink(target, path, "file").then(
    () => true,
    (error: NodeJS.ErrnoException) => {
      if (process.platform === "win32" && error.code === "EPERM") return false;
      throw error;
    },
  );
}

function runSprawlGuard(cwd: string, args: readonly string[], expectedStatus = 0): string {
  const [command, ...prefix] = sprawlGuard;
  return run(cwd, command, [...prefix, ...args], expectedStatus);
}

function run(cwd: string, command: string, args: readonly string[], expectedStatus = 0): string {
  const { error, status, stdout, stderr } = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    env,
  });
  if (error) throw error;
  assert.equal(status, expectedStatus, `${command} ${args.join(" ")}\n${stdout}${stderr}`);
  return stdout;
}
