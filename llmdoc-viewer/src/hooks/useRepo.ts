import { useState, useEffect, useCallback } from "react"
import type { RepoState, TreeNode, GitNode } from "@/types"
import {
  fetchRepoInfo,
  fetchRepoTree,
  filterLLMDocs,
  buildTreeStructure,
  fetchFileContent,
} from "@/lib/github"
import {
  getHomeFileContent,
  getHomeRepoTree,
  isHomeRepo,
} from "@/lib/homeDocs"

interface UseRepoOptions {
  owner: string
  repo: string
  token?: string
}

interface UseRepoReturn extends RepoState {
  tree: TreeNode[]
  selectedFile?: string
  fileContent?: string
  isLoadingFile: boolean
  selectFile: (path: string, sha: string) => void
  tabs: {
    claudeMd?: GitNode
    agentsMd?: GitNode
    docsTree: GitNode[]
  }
}

export function useRepo({ owner, repo, token }: UseRepoOptions): UseRepoReturn {
  const [state, setState] = useState<RepoState>({
    isLoading: true,
    hasContent: false,
    tabs: { docsTree: [] },
  })

  const [tree, setTree] = useState<TreeNode[]>([])
  const [selectedFile, setSelectedFile] = useState<string>()
  const [selectedSha, setSelectedSha] = useState<string>()
  const [fileContent, setFileContent] = useState<string>()
  const [isLoadingFile, setIsLoadingFile] = useState(false)

  // 加载仓库数据
  useEffect(() => {
    let cancelled = false

    async function loadRepo() {
      setState((s) => ({ ...s, isLoading: true, error: undefined }))

      try {
        const fullTree = isHomeRepo(owner, repo)
          ? getHomeRepoTree()
          : await (async () => {
              const options = token ? { token } : undefined
              const repoInfo = await fetchRepoInfo(owner, repo, options)
              return fetchRepoTree(owner, repo, repoInfo.defaultBranch, options)
            })()

        // 过滤 LLM 文档
        const filtered = filterLLMDocs(fullTree)

        // 构建树结构
        const treeStructure = buildTreeStructure(filtered.docsTree)

        if (cancelled) return

        const hasContent =
          !!filtered.claudeMd ||
          !!filtered.agentsMd ||
          filtered.docsTree.length > 0

        setState({
          isLoading: false,
          hasContent,
          tabs: filtered,
        })
        setTree(treeStructure)
      } catch (error) {
        if (cancelled) return

        const errorMessage =
          error instanceof Error ? error.message : "Unknown error"

        setState({
          isLoading: false,
          hasContent: false,
          error: errorMessage,
          tabs: { docsTree: [] },
        })
      }
    }

    loadRepo()

    return () => {
      cancelled = true
    }
  }, [owner, repo, token])

  // 加载文件内容
  useEffect(() => {
    if (!selectedSha || !selectedFile) {
      setFileContent(undefined)
      return
    }

    const currentSelectedFile = selectedFile
    const currentSelectedSha = selectedSha
    let cancelled = false

    async function loadFile() {
      setIsLoadingFile(true)
      try {
        const content = isHomeRepo(owner, repo)
          ? getHomeFileContent(currentSelectedFile)
          : await fetchFileContent(
              owner,
              repo,
              currentSelectedSha,
              token ? { token } : undefined
            )

        if (!content) {
          throw new Error("FILE_NOT_FOUND")
        }

        if (!cancelled) {
          setFileContent(content)
        }
      } catch (error) {
        console.error("Failed to load file:", error)
        if (!cancelled) {
          setFileContent("Failed to load file content")
        }
      } finally {
        if (!cancelled) {
          setIsLoadingFile(false)
        }
      }
    }

    loadFile()

    return () => {
      cancelled = true
    }
  }, [owner, repo, selectedFile, selectedSha, token])

  const selectFile = useCallback((path: string, sha: string) => {
    setSelectedFile(path)
    setSelectedSha(sha)
  }, [])

  return {
    ...state,
    tree,
    selectedFile,
    fileContent,
    isLoadingFile,
    selectFile,
  }
}
