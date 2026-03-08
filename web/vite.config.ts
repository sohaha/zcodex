import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { resolve } from 'node:path'

const base = process.env.VITE_BASE_URL || '/'
const backend = process.env.VITE_BACKEND_URL || 'http://127.0.0.1:3006'
const appVersion = process.env.VITE_APP_VERSION || 'dev'

export default defineConfig({
    define: {
        __APP_VERSION__: JSON.stringify(appVersion),
    },
    server: {
        host: true,
        proxy: {
            '/api': {
                target: backend,
                changeOrigin: true
            },
            '/ws': {
                target: backend,
                ws: true
            }
        }
    },
    plugins: [
        react(),
    ],
    base,
    resolve: {
        alias: {
            '@': resolve(__dirname, 'src')
        }
    },
    build: {
        outDir: 'dist',
        emptyOutDir: true,
        rollupOptions: {
            output: {
                manualChunks(id) {
                    if (!id.includes('node_modules')) {
                        return undefined
                    }
                    if (id.includes('/@xterm/')) {
                        return 'terminal-vendor'
                    }
                    if (
                        id.includes('/shiki/')
                        || id.includes('/remark-gfm/')
                        || id.includes('/hast-util-to-jsx-runtime/')
                        || id.includes('/@assistant-ui/react-markdown/')
                    ) {
                        return 'markdown-vendor'
                    }
                    if (id.includes('/@elevenlabs/')) {
                        return 'voice-vendor'
                    }
                    if (id.includes('/@assistant-ui/react/')) {
                        return 'assistant-ui-vendor'
                    }
                    return undefined
                }
            }
        }
    }
})
