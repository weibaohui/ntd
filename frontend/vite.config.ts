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
        // 093：manualChunks 从对象式改为函数式。
        // 对象式会把被列模块的关联模块（含 Rollup 共享 helper）合并进 vendor chunk，
        // 实测入口 chunk 出现 `import{_ as we}from"./vendor-monaco-*.js"`，导致首屏
        // 被迫 preload 3.3MB monaco；函数式只按模块路径精确归类、不吞并依赖，
        // helper 与未被匹配的依赖回退到 Rollup 按 lazy 边界自然分包。
        manualChunks(id) {
          // Vite 注入的 modulepreload helper（__vitePreload，id 形如 \0vite/preload-helper.js）
          // 是入口与所有 lazy chunk 的共同依赖。不显式归类时 Rollup 会把它随手并入某个
          // vendor chunk（实测落进 3.8MB 的 vendor-monaco），入口为引这一个 helper 被迫
          // preload 整个 vendor。显式锚定到独立小 chunk，入口只 preload 几 KB。
          if (id.includes('vite/preload-helper')) return 'vendor-runtime';
          // 项目源码一律返回 undefined，让 React.lazy 的动态边界决定分包；
          // 只有 node_modules 里的第三方库才做命名 vendor 归类。
          if (!id.includes('node_modules')) return undefined;
          // React 运行时必须显式锚定到独立 chunk。不指定时 Rollup 会把 react 自然并入
          // 最大引用方 vendor-antd，而 react-icons 等库顶层执行 `React.createContext()`，
          // chunk 间形成循环依赖后模块尚未初始化就读取 → 运行时
          // "Cannot read properties of undefined (reading 'createContext')"，应用白屏。
          // 路径按目录边界精确匹配，'/node_modules/react/' 不会误伤 react-icons 等同缀包。
          if (/\/node_modules\/(react|react-dom|scheduler|react-is)\//.test(id)) return 'vendor-react';
          // monaco 体量最大（核心+各语言子包 ~3.3MB），仅工艺 YAML 编辑器经
          // 动态 import('monaco-editor') 按需加载，锚定到稳定命名 chunk 防回潮。
          if (id.includes('/monaco-editor/')) return 'vendor-monaco';
          // @xyflow/react（React Flow）+ dagre 布局仅工艺可视化页使用。
          if (id.includes('/@xyflow/') || id.includes('/dagre/')) return 'vendor-flow';
          // markdown 渲染/编辑器经 093 懒加载后按需加载；x-markdown 与 md-editor
          // 共用一份 markdown 依赖树，归同一 chunk 避免重复下载。
          // 注意必须先于下方 antd 广义匹配返回（x-markdown 路径也含 '@ant-design'）。
          if (id.includes('/@ant-design/x-markdown/')) return 'vendor-md-editor';
          if (id.includes('/@uiw/react-md-editor/') || id.includes('/@uiw/react-markdown-preview/')) return 'vendor-md-editor';
          // antd 全家桶（含 @ant-design/icons、cssinjs、rc-*、@rc-component/*）必须归
          // 同一个 chunk：antd ⇄ icons ⇄ cssinjs 相互引用，拆开后 chunk 间形成循环依赖，
          // 模块未初始化就被读取 → 运行时 TDZ 报错白屏（本分支实测验证）。
          if (id.includes('/antd/') || id.includes('/@ant-design/') || id.includes('/rc-') || id.includes('/@rc-component/')) return 'vendor-antd';
          if (id.includes('/react-icons/')) return 'vendor-icons';
          // 小组件库按使用面归堆：qrcode（设置页）、react-countup（Dashboard）、
          // react-js-cron（调度 UI）各自体积小，合并减少请求数。
          if (id.includes('/qrcode/') || id.includes('/react-countup/') || id.includes('/react-js-cron/')) return 'vendor-misc';
          // 其余第三方依赖（react/react-dom/axios/dayjs 等）不强制归类，
          // 由 Rollup 按静态/动态引用关系自然落 chunk，避免 helper 提升问题复发。
          return undefined;
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
