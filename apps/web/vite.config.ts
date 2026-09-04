import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    // Dev talks to the Rust API on 8080; in production VITE_API_URL points at
    // the deployed API service instead.
    proxy: {
      '/api': {
        target: process.env.VITE_API_PROXY ?? 'http://localhost:8080',
        changeOrigin: true,
      },
      // Uploaded hero/gallery images are served from the API too, at a
      // plain (non-/api) path — see services/api/src/site_content.rs.
      '/uploads': {
        target: process.env.VITE_API_PROXY ?? 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
});
