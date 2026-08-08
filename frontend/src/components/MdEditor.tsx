import { lazy, Suspense } from 'react';
import { Spin } from 'antd';
import { useTheme } from '@/hooks/useTheme';

// 093：@uiw/react-md-editor（vendor-md-editor chunk 的另一半来源，gzip 前 ~600KB）
// 原本是顶层静态 import，经 TodoDrawer → PromptEditor 静态链锚定进首屏。
// 改为封装层内部 lazy：3 个调用方（PromptEditor / DiscussionComposer /
// PromptMdField）零改动，编辑器 chunk 只在用户真正打开编辑界面时才下载。
const MDEditor = lazy(() => import('@uiw/react-md-editor'));

interface MdEditorProps {
  value: string;
  onChange: (value: string) => void;
  height?: number | string;
  /** 暴露 editor ref，可用于获取 textarea 和光标位置 */
  editorRef?: React.RefObject<any>;
}

export function MdEditor({
  value,
  onChange,
  height,
  editorRef,
}: MdEditorProps) {
  const { themeMode } = useTheme();

  // 高度解析提前算好，让 Suspense fallback 占位与编辑器最终高度一致，
  // 避免 chunk 加载完成瞬间抽屉/表单布局跳动（CLS）。
  const resolvedMinHeight = typeof height === 'number' ? height : (height || '100%');

  return (
    <div data-color-mode={themeMode} style={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
      <Suspense
        fallback={
          // 编辑器是重交互组件，加载一瞬用 Spin 表达「正在就绪」比纯文本更贴切；
          // minHeight 与最终渲染一致，占位只影响视觉不影响布局。
          <div style={{ flex: 1, minHeight: resolvedMinHeight, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <Spin />
          </div>
        }
      >
        <MDEditor
          value={value}
          onChange={(val) => onChange(val || '')}
          preview="edit"
          style={{ flex: 1, minHeight: resolvedMinHeight }}
          // React 19 下函数组件 ref 走 prop 透传，lazy 包装不破坏 ref 转发链。
          ref={editorRef}
        />
      </Suspense>
    </div>
  );
}
