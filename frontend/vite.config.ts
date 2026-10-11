import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

// The application is served under /app by the Rust backend (production)
// or by this dev server behind the development proxy.
export default defineConfig({
  base: '/app/',
  plugins: [react(), tailwindcss()],
  server: {
    // NW_DEV_HOST=0.0.0.0 lets the development proxy (Docker) reach Vite.
    host: process.env.NW_DEV_HOST ?? '127.0.0.1',
    port: 5173,
    proxy: {
      '/api': process.env.NW_API_URL ?? 'http://127.0.0.1:8080',
    },
  },
  test: {
    environment: 'node',
  },
})
