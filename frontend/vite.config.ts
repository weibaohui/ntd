import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath, URL } from "node:url";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  build: {
    outDir: 'dist',
    assetsInlineLimit: 4096,
    chunkSizeWarningLimit: 500,
    rollupOptions: {
      output: {
        manualChunks: {
          // react / react-dom 不单独拆：Vite 默认把它们打入主 chunk，
          // 这里留 manualChunks 占位会产生 0 KB 的空 vendor-react 文件。
          // 真正需要拆分的是 antd / 图标 / markdown 编辑器这种大型第三方库。
          'vendor-antd': ['antd'],
          'vendor-antd-icons': ['@ant-design/icons'],
          'vendor-md-editor': ['@uiw/react-md-editor', '@ant-design/x-markdown'],

          'vendor-icons': ['react-icons'],
          'vendor-misc': ['qrcode', 'react-countup', 'react-js-cron'],
        },
      },
    },
  },
  server: {
    port: 5173,
    strictPort: false,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:18088',
        changeOrigin: true,
        ws: true,
      },
    },
  },
});
