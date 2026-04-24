#!/usr/bin/env node

import { spawn } from "node:child_process"
import { existsSync, realpathSync } from "fs"
import { createRequire } from "node:module"
import path from "path"
import { fileURLToPath } from "url"

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const require = createRequire(import.meta.url)

const MAIN_PACKAGE_NAME = "@sohaha/zcodex"
const PACKAGE_PREFIX = "@sohaha/zcodex"

const PLATFORM_PACKAGE_BY_TARGET = {
  "x86_64-unknown-linux-musl": `${PACKAGE_PREFIX}-linux-x64`,
  "aarch64-unknown-linux-musl": `${PACKAGE_PREFIX}-linux-arm64`,
  "x86_64-apple-darwin": `${PACKAGE_PREFIX}-darwin-x64`,
  "aarch64-apple-darwin": `${PACKAGE_PREFIX}-darwin-arm64`,
  "x86_64-pc-windows-msvc": `${PACKAGE_PREFIX}-win32-x64`,
  "aarch64-pc-windows-msvc": `${PACKAGE_PREFIX}-win32-arm64`,
}

function getUpdatedPath(newDirs) {
  const pathSep = process.platform === "win32" ? ";" : ":"
  const existingPath = process.env.PATH || ""
  const updatedPath = [
    ...newDirs,
    ...existingPath.split(pathSep).filter(Boolean),
  ].join(pathSep)
  return updatedPath
}

function detectPackageManager() {
  const userAgent = process.env.npm_config_user_agent || ""
  if (/\bbun\//.test(userAgent)) {
    return "bun"
  }

  const execPath = process.env.npm_execpath || ""
  if (execPath.includes("bun")) {
    return "bun"
  }

  if (
    __dirname.includes(".bun/install/global") ||
    __dirname.includes(".bun\\install\\global")
  ) {
    return "bun"
  }

  return userAgent ? "npm" : null
}

function getUpdateSuggestion() {
  const packageManager = detectPackageManager()
  return packageManager === "bun"
    ? "bun install -g @sohaha/zcodex@latest"
    : "npm install -g @sohaha/zcodex@latest"
}

export function sanitizeRipgrepConfig(env) {
  const configPath = env.RIPGREP_CONFIG_PATH
  if (!configPath) {
    return env
  }

  if (existsSync(configPath)) {
    return env
  }

  const { RIPGREP_CONFIG_PATH, ...others } = env
  return others
}

function resolveTargetTriple() {
  const { platform, arch } = process
  let targetTriple = null

  switch (platform) {
    case "linux":
    case "android":
      switch (arch) {
        case "x64":
          targetTriple = "x86_64-unknown-linux-musl"
          break
        case "arm64":
          targetTriple = "aarch64-unknown-linux-musl"
          break
        default:
          break
      }
      break
    case "darwin":
      switch (arch) {
        case "x64":
          targetTriple = "x86_64-apple-darwin"
          break
        case "arm64":
          targetTriple = "aarch64-apple-darwin"
          break
        default:
          break
      }
      break
    case "win32":
      switch (arch) {
        case "x64":
          targetTriple = "x86_64-pc-windows-msvc"
          break
        case "arm64":
          targetTriple = "aarch64-pc-windows-msvc"
          break
        default:
          break
      }
      break
    default:
      break
  }

  if (!targetTriple) {
    throw new Error(`不支持的平台: ${platform} (${arch})`)
  }

  return targetTriple
}

function resolveMainModuleState() {
  const scriptPath = path.resolve(fileURLToPath(import.meta.url))
  if (!process.argv[1]) {
    return false
  }

  const invokedPath = path.resolve(process.argv[1])
  if (invokedPath === scriptPath) {
    return true
  }

  if (!existsSync(invokedPath)) {
    return false
  }

  return realpathSync(invokedPath) === realpathSync(scriptPath)
}

const isMainModule = resolveMainModuleState()

function resolveVendorRoot(platformPackage, localBinaryPath) {
  try {
    const packageJsonPath = require.resolve(`${platformPackage}/package.json`)
    return { vendorRoot: path.join(path.dirname(packageJsonPath), "vendor"), error: null }
  } catch (firstError) {
    const fallbackPaths = []
    
    try {
      const mainPackagePath = require.resolve(`${MAIN_PACKAGE_NAME}/package.json`)
      const mainPackageDir = path.dirname(mainPackagePath)
      fallbackPaths.push(path.join(mainPackageDir, "..", platformPackage, "package.json"))
    } catch (e) {
    }
    
    try {
      const mainPackagePath = require.resolve(`${MAIN_PACKAGE_NAME}/package.json`)
      const mainPackageDir = path.dirname(mainPackagePath)
      fallbackPaths.push(path.join(mainPackageDir, "node_modules", platformPackage, "package.json"))
    } catch (e) {
    }
    
    try {
      const globalPaths = require('module').globalPaths
      for (const globalPath of globalPaths) {
        fallbackPaths.push(path.join(globalPath, MAIN_PACKAGE_NAME, "node_modules", platformPackage, "package.json"))
      }
    } catch (e) {
    }
    
    for (const fallbackPath of fallbackPaths) {
      if (existsSync(fallbackPath)) {
        return { vendorRoot: path.join(path.dirname(fallbackPath), "vendor"), error: null }
      }
    }
    
    if (existsSync(localBinaryPath)) {
      return { vendorRoot: path.join(__dirname, "..", "vendor"), error: null }
    }
    
    return { vendorRoot: null, error: firstError }
  }
}

async function main() {
  const targetTriple = resolveTargetTriple()
  const platformPackage = PLATFORM_PACKAGE_BY_TARGET[targetTriple]
  if (!platformPackage) {
    throw new Error(`不支持的 target triple: ${targetTriple}`)
  }

  const codexBinaryName = process.platform === "win32" ? "codex.exe" : "codex"
  const localVendorRoot = path.join(__dirname, "..", "vendor")
  const localBinaryPath = path.join(
    localVendorRoot,
    targetTriple,
    "codex",
    codexBinaryName,
  )

  const { vendorRoot, error } = resolveVendorRoot(platformPackage, localBinaryPath)

  if (!vendorRoot) {
    const updateCommand = getUpdateSuggestion()
    throw new Error(
      `缺少可选依赖 ${platformPackage}。请重新安装 Codex: ${updateCommand}\n原始错误: ${error ? error.message : 'unknown'}`,
    )
  }

  const archRoot = path.join(vendorRoot, targetTriple)
  const binaryPath = path.join(archRoot, "codex", codexBinaryName)

  const additionalDirs = []
  const pathDir = path.join(archRoot, "path")
  if (existsSync(pathDir)) {
    additionalDirs.push(pathDir)
  }
  const updatedPath = getUpdatedPath(additionalDirs)

  const baseEnv = { ...process.env, PATH: updatedPath }
  const packageManagerEnvVar =
    detectPackageManager() === "bun"
      ? "CODEX_MANAGED_BY_BUN"
      : "CODEX_MANAGED_BY_NPM"
  baseEnv[packageManagerEnvVar] = "1"

  const env = sanitizeRipgrepConfig(baseEnv)

  const child = spawn(binaryPath, process.argv.slice(2), {
    stdio: "inherit",
    env,
  })

  child.on("error", (err) => {
    console.error(err)
    process.exit(1)
  })

  const forwardSignal = (signal) => {
    if (child.killed) {
      return
    }
    try {
      child.kill(signal)
    } catch {
    }
  };

  ["SIGINT", "SIGTERM", "SIGHUP"].forEach((sig) => {
    process.on(sig, () => forwardSignal(sig))
  })

  const childResult = await new Promise((resolve) => {
    child.on("exit", (code, signal) => {
      if (signal) {
        resolve({ type: "signal", signal })
      } else {
        resolve({ type: "code", exitCode: code ?? 1 })
      }
    })
  })

  if (childResult.type === "signal") {
    process.kill(process.pid, childResult.signal)
  } else {
    process.exit(childResult.exitCode)
  }
}

if (isMainModule) {
  main().catch((err) => {
    console.error(err)
    process.exit(1)
  })
}
