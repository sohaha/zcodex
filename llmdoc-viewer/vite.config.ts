import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

function spaFallbackForDocs() {
  const rewriteHtmlRequest = (url: string | undefined, acceptHeader: string | undefined) => {
    if (!url || !acceptHeader?.includes('text/html')) {
      return undefined
    }

    if (url.startsWith('/api/')) {
      return undefined
    }

    return '/index.html'
  }

  return {
    name: 'spa-fallback-for-doc-routes',
    configureServer(server: { middlewares: { use: (fn: (req: { url?: string; headers: Record<string, string | string[] | undefined> }, _res: unknown, next: () => void) => void) => void } }) {
      server.middlewares.use((req, _res, next) => {
        const acceptHeader = Array.isArray(req.headers.accept)
          ? req.headers.accept.join(',')
          : req.headers.accept
        const rewrittenUrl = rewriteHtmlRequest(req.url, acceptHeader)

        if (rewrittenUrl) {
          req.url = rewrittenUrl
        }

        next()
      })
    },
    configurePreviewServer(server: { middlewares: { use: (fn: (req: { url?: string; headers: Record<string, string | string[] | undefined> }, _res: unknown, next: () => void) => void) => void } }) {
      server.middlewares.use((req, _res, next) => {
        const acceptHeader = Array.isArray(req.headers.accept)
          ? req.headers.accept.join(',')
          : req.headers.accept
        const rewrittenUrl = rewriteHtmlRequest(req.url, acceptHeader)

        if (rewrittenUrl) {
          req.url = rewrittenUrl
        }

        next()
      })
    },
  }
}

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), spaFallbackForDocs()],
  server: {
    allowedHosts: ['swift-canyon-e0fb9bc9.tunnl.gg', 'ab8c0951038997.lhr.life'],
    fs: {
      allow: [path.resolve(__dirname, '..')],
    },
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
})
