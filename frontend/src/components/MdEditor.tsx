import { useTheme } from '../hooks/useTheme';
import MDEditor from '@uiw/react-md-editor';

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

  return (
    <div data-color-mode={themeMode} style={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
      <MDEditor
        value={value}
        onChange={(val) => onChange(val || '')}
        preview="edit"
        style={{ flex: 1, minHeight: typeof height === 'number' ? height : (height || '100%') }}
        ref={editorRef}
      />
    </div>
  );
}