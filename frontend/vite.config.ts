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
          // 091：monaco-editor 体量巨大（核心 + 各语言子包），且仅工艺 YAML 编辑器使用。
          // ProcessYamlEditor 已改为动态 import('monaco-editor')，这里再用 manualChunks
          // 把它锚定到稳定命名 chunk，确保不被并入主 bundle 或工艺页 chunk。
          'vendor-monaco': ['monaco-editor'],
          // 091：@xyflow/react（React Flow）+ dagre 布局仅工艺可视化页使用，独立成 chunk。
          'vendor-flow': ['@xyflow/react', 'dagre'],
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
