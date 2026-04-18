import type { GitNode } from "@/types"

export const HOME_ROUTE_OWNER = "__local"
export const HOME_ROUTE_REPO = "home"

const homeAgentsMd = import.meta.glob("../../../AGENTS.md", {
  eager: true,
  import: "default",
  query: "?raw",
}) as Record<string, string>

const homeClaudeMd = import.meta.glob("../../../CLAUDE.md", {
  eager: true,
  import: "default",
  query: "?raw",
}) as Record<string, string>

const homeLlmsTxt = import.meta.glob("../../../llms.txt", {
  eager: true,
  import: "default",
  query: "?raw",
}) as Record<string, string>

const homeLlmdocFiles = import.meta.glob("../../../.agents/llmdoc/**/*.md", {
  eager: true,
  import: "default",
  query: "?raw",
}) as Record<string, string>

function normalizeHomePath(globPath: string): string | undefined {
  if (globPath.endsWith("/AGENTS.md")) {
    return "agents.md"
  }

  if (globPath.endsWith("/CLAUDE.md")) {
    return "claude.md"
  }

  if (globPath.endsWith("/llms.txt")) {
    return "llms.txt"
  }

  const marker = "/.agents/llmdoc/"
  const markerIndex = globPath.indexOf(marker)
  if (markerIndex === -1) {
    return undefined
  }

  const relativePath = globPath.slice(markerIndex + marker.length)
  return `llmdoc/${relativePath}`
}

function collectHomeFileContents(): Record<string, string> {
  const fileContents: Record<string, string> = {}
  const sources = [homeAgentsMd, homeClaudeMd, homeLlmsTxt, homeLlmdocFiles]

  for (const source of sources) {
    for (const [globPath, content] of Object.entries(source)) {
      const normalizedPath = normalizeHomePath(globPath)
      if (normalizedPath) {
        fileContents[normalizedPath] = content
      }
    }
  }

  return fileContents
}

function createTreeNode(path: string): GitNode {
  return {
    path,
    mode: "040000",
    type: "tree",
    sha: `local:${path}`,
    url: `local://${path}`,
  }
}

function createBlobNode(path: string): GitNode {
  return {
    path,
    mode: "100644",
    type: "blob",
    sha: `local:${path}`,
    url: `local://${path}`,
  }
}

function collectHomeTree(filePaths: string[]): GitNode[] {
  const folderPaths = new Set<string>()

  for (const filePath of filePaths) {
    const parts = filePath.split("/")
    let currentPath = ""

    for (const part of parts.slice(0, -1)) {
      currentPath = currentPath ? `${currentPath}/${part}` : part
      folderPaths.add(currentPath)
    }
  }

  return [
    ...[...folderPaths].sort((a, b) => a.localeCompare(b)).map(createTreeNode),
    ...filePaths.sort((a, b) => a.localeCompare(b)).map(createBlobNode),
  ]
}

const HOME_FILE_CONTENTS = collectHomeFileContents()
const HOME_TREE = collectHomeTree(Object.keys(HOME_FILE_CONTENTS))

export function isHomeRepo(owner: string, repo: string): boolean {
  return owner === HOME_ROUTE_OWNER && repo === HOME_ROUTE_REPO
}

export function getHomeRouteParams() {
  return {
    owner: HOME_ROUTE_OWNER,
    repo: HOME_ROUTE_REPO,
    _splat: "",
  }
}

export function getHomeRepoTree(): GitNode[] {
  return HOME_TREE
}

export function getHomeFileContent(path: string): string | undefined {
  return HOME_FILE_CONTENTS[path]
}
