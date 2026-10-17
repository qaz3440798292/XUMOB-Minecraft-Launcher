import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync } from 'node:fs';
import { basename, delimiter, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const targetDir = join(root, 'src-tauri', 'target');
const cargoBinaryName = 'xumob-minecraft-launcher.exe';
const outputBinaryName = 'XUMOB Minecraft Launcher.exe';
const launcherBinaryNames = new Set([cargoBinaryName, outputBinaryName.toLowerCase()]);
const tauriCli = join(root, 'node_modules', '@tauri-apps', 'cli', 'tauri.js');
const args = process.argv.slice(2);

// A private local file supports release-time injection without tracking secrets.
const privateBuildEnv = join(root, '.env.release');
if (existsSync(privateBuildEnv)) {
  for (const line of readFileSync(privateBuildEnv, 'utf8').split(/\r?\n/)) {
    const match = line.match(/^\s*(XUMOB_BUILD_(?:CURSEFORGE_API_KEY|MICROSOFT_CLIENT_ID))\s*=\s*(.*?)\s*$/);
    if (!match || process.env[match[1]]) continue;
    process.env[match[1]] = match[2].replace(/^(['"])(.*)\1$/, '$2');
  }
}

function findLocalGnuRuntime() {
  const candidates = [];
  if (process.env.XUMOB_BUILD_RUNTIME) {
    candidates.push(resolve(process.env.XUMOB_BUILD_RUNTIME));
  }
  if (process.env.CARGO_HOME && basename(process.env.CARGO_HOME).toLowerCase() === 'cargo') {
    candidates.push(dirname(resolve(process.env.CARGO_HOME)));
  }
  for (const entry of (process.env.PATH ?? '').split(delimiter)) {
    const directory = resolve(entry.replace(/^"|"$/g, ''));
    if (
      basename(directory).toLowerCase() === 'bin' &&
      basename(dirname(directory)).toLowerCase() === 'cargo'
    ) {
      candidates.push(dirname(dirname(directory)));
    }
  }

  for (const runtime of new Set(candidates)) {
    const cargoHome = join(runtime, 'cargo');
    const rustupHome = join(runtime, 'rustup');
    const toolchain = join(rustupHome, 'toolchains', 'stable-x86_64-pc-windows-gnu');
    if (
      !existsSync(join(cargoHome, 'bin', 'cargo.exe')) ||
      !existsSync(join(toolchain, 'bin', 'rustc.exe'))
    ) {
      continue;
    }

    const compilerRoot = readdirSync(runtime, { withFileTypes: true })
      .filter((entry) => entry.isDirectory() && entry.name.startsWith('llvm-mingw'))
      .map((entry) => join(runtime, entry.name, 'bin'))
      .find((directory) => existsSync(join(directory, 'x86_64-w64-mingw32-gcc.exe')));
    const webview2StaticDir = join(dirname(runtime), 'webview2-static');
    if (
      !compilerRoot ||
      !existsSync(join(webview2StaticDir, 'webview2_crt_shim.o')) ||
      !existsSync(join(webview2StaticDir, 'lib', 'libWebView2LoaderStatic.a'))
    ) {
      continue;
    }
    return { cargoHome, rustupHome, toolchain, compilerRoot, webview2StaticDir };
  }
  return null;
}

const localGnuRuntime = findLocalGnuRuntime();
if (localGnuRuntime) {
  const { cargoHome, rustupHome, toolchain, compilerRoot, webview2StaticDir } = localGnuRuntime;
  const selfContained = join(
    toolchain,
    'lib',
    'rustlib',
    'x86_64-pc-windows-gnu',
    'lib',
    'self-contained',
  );
  const gcc = join(compilerRoot, 'x86_64-w64-mingw32-gcc.exe');
  const dlltool = join(compilerRoot, 'dlltool.exe');

  process.env.CARGO_HOME = cargoHome;
  process.env.RUSTUP_HOME = rustupHome;
  process.env.RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-gnu';
  process.env.CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = gcc;
  process.env.LIBRARY_PATH = selfContained;
  process.env.RUSTFLAGS = [process.env.RUSTFLAGS, `-C dlltool=${dlltool}`]
    .filter(Boolean)
    .join(' ');
  process.env.CC = gcc;
  process.env.AR = join(compilerRoot, 'llvm-ar.exe');
  process.env.WINDRES = join(compilerRoot, 'llvm-windres.exe');
  process.env.CARGO_BUILD_JOBS ||= '4';
  process.env.XUMOB_WEBVIEW2_STATIC_DIR ||= webview2StaticDir;
  process.env.PATH = [
    compilerRoot,
    join(selfContained, 'bin'),
    join(cargoHome, 'bin'),
    process.env.PATH,
  ].join(delimiter);
  console.log('已检测到本机 GNU Rust 工具链，使用它构建 Windows 版本。');
}

if (!existsSync(tauriCli)) {
  console.error('找不到本地 Tauri CLI，请先运行 npm ci。');
  process.exit(1);
}

// Force Tauri and Cargo to keep every Rust build artifact beside this project.
process.env.CARGO_TARGET_DIR = targetDir;

function buildMsalBroker() {
  if (process.platform !== 'win32') return true;

  const localDotnet = join(root, '.tools', 'dotnet', 'dotnet.exe');
  const dotnet = existsSync(localDotnet) ? localDotnet : 'dotnet';
  const sdk = spawnSync(dotnet, ['--list-sdks'], { cwd: root, encoding: 'utf8' });
  if (sdk.error || sdk.status !== 0 || !sdk.stdout.trim()) {
    console.error('构建 Windows 版本需要已安装 .NET 8 SDK，以编译 MSAL Windows 登录组件。');
    return false;
  }

  if (existsSync(localDotnet)) {
    process.env.DOTNET_ROOT = join(root, '.tools', 'dotnet');
    process.env.DOTNET_CLI_HOME = join(root, '.tools', 'cli-home');
    process.env.NUGET_PACKAGES = join(root, '.tools', 'nuget');
    process.env.APPDATA = join(root, '.tools', 'appdata');
    process.env.LOCALAPPDATA = join(root, '.tools', 'localappdata');
    for (const directory of [process.env.DOTNET_CLI_HOME, process.env.NUGET_PACKAGES, process.env.APPDATA, process.env.LOCALAPPDATA]) {
      mkdirSync(directory, { recursive: true });
    }
  }

  const profile = args[0] === 'dev' ? 'debug' : 'release';
  const profileDir = join(targetDir, profile);
  const helperOutput = join(profileDir, 'msal-broker');
  mkdirSync(profileDir, { recursive: true });
  const publish = spawnSync(
    dotnet,
    [
      'publish',
      join(root, 'src-tauri', 'msal-broker', 'Xumob.MsalBroker.csproj'),
      '--configuration',
      profile === 'debug' ? 'Debug' : 'Release',
      '--runtime',
      'win-x64',
      '--self-contained',
      'true',
      '--output',
      helperOutput,
    ],
    { cwd: root, env: process.env, stdio: 'inherit' },
  );
  if (publish.error || publish.status !== 0) {
    console.error('无法构建 MSAL Windows 登录组件。');
    if (publish.error) console.error(publish.error.message);
    return false;
  }

  return true;
}

if (!buildMsalBroker()) process.exit(1);

function findLauncherBinaries(directory) {
  if (!existsSync(directory)) return [];

  const matches = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory() && !entry.isSymbolicLink()) {
      matches.push(...findLauncherBinaries(path));
    } else if (entry.isFile() && launcherBinaryNames.has(entry.name.toLowerCase())) {
      matches.push(path);
    }
  }
  return matches;
}

const isReleaseBuild = args[0] === 'build';
if (isReleaseBuild) {
  const oldBinaries = findLauncherBinaries(targetDir);
  try {
    for (const binary of oldBinaries) rmSync(binary);
  } catch (error) {
    console.error('无法清理旧启动器。请先关闭正在运行的 XUMOB Minecraft Launcher，再重新构建。');
    console.error(error.message);
    process.exit(1);
  }
  if (oldBinaries.length) console.log(`已清理 ${oldBinaries.length} 个旧启动器程序文件。`);
}

const result = spawnSync(process.execPath, [tauriCli, ...args], {
  cwd: root,
  env: process.env,
  stdio: 'inherit',
});

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}
if (result.status !== 0) process.exit(result.status ?? 1);

if (isReleaseBuild) {
  const outputs = findLauncherBinaries(targetDir);
  if (outputs.length !== 1) {
    console.error(`构建已结束，但预期只找到一个启动器程序，实际找到 ${outputs.length} 个。`);
    process.exit(1);
  }
  const output = join(dirname(outputs[0]), outputBinaryName);
  try {
    renameSync(outputs[0], output);
  } catch (error) {
    console.error('无法将构建产物改为启动器名称。');
    console.error(error.message);
    process.exit(1);
  }
  console.log(`当前版本已生成：${output}`);
}
