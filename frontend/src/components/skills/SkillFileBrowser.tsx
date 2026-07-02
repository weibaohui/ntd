import { useState, useMemo } from 'react';
import { Input, Spin, Typography, Empty } from 'antd';
import {
  SearchOutlined, FileOutlined, FolderOutlined, FolderOpenOutlined,
} from '@ant-design/icons';
import type { SkillFileInfo } from '@/utils/database/skills';
import { formatSize, getFileColor } from './helpers';

const { Text } = Typography;

interface SkillFileBrowserProps {
  files: SkillFileInfo[];
  loading?: boolean;
  onFileSelect?: (file: SkillFileInfo) => void;
  selectedFile?: SkillFileInfo | null;
  isDark?: boolean;
}

interface FileTreeNode {
  name: string;
  path: string;
  isDir: boolean;
  children: FileTreeNode[];
  file?: SkillFileInfo;
}

// 将文件列表按路径层级构建为树结构，支持目录嵌套展示。
// 使用线性查找而非 Map：文件数量通常不多（<100），Map 初始化成本不划算；
// 同时保持节点顺序与文件列表顺序一致，利于调试和人类阅读。
function buildFileTree(files: SkillFileInfo[]): FileTreeNode {
  const root: FileTreeNode = {
    name: '/',
    path: '',
    isDir: true,
    children: [],
  };

  files.forEach(file => {
    const parts = file.path.split('/').filter(Boolean);
    let current = root;

    parts.forEach((part, index) => {
      const isLast = index === parts.length - 1;
      const existingChild = current.children.find(c => c.name === part);

      if (existingChild) {
        if (isLast) {
          existingChild.file = file;
          existingChild.isDir = false;
        }
        current = existingChild;
      } else {
        const newNode: FileTreeNode = {
          name: part,
          path: parts.slice(0, index + 1).join('/'),
          isDir: !isLast,
          children: [],
          file: isLast ? file : undefined,
        };
        current.children.push(newNode);
        current = newNode;
      }
    });
  });

  return root;
}

// 根据搜索词递归过滤文件树，保留匹配的目录路径（即使目录本身不匹配，只要子节点有匹配就保留）。
// 目录名匹配时保留整棵子树；文件名校验大小写不敏感。
// 返回 null 表示该节点及所有子节点均不匹配，可直接跳过。
function filterFileTree(node: FileTreeNode, searchText: string): FileTreeNode | null {
  if (!searchText) return node;

  const lowerSearch = searchText.toLowerCase();
  const filteredChildren: FileTreeNode[] = [];

  node.children.forEach(child => {
    if (child.isDir) {
      const filteredChild = filterFileTree(child, searchText);
      if (filteredChild && filteredChild.children.length > 0) {
        filteredChildren.push(filteredChild);
      }
    } else if (child.name.toLowerCase().includes(lowerSearch)) {
      filteredChildren.push(child);
    }
  });

  if (filteredChildren.length === 0 && !node.name.toLowerCase().includes(lowerSearch)) {
    return null;
  }

  return {
    ...node,
    children: filteredChildren,
  };
}

// 递归渲染文件树节点，支持展开/折叠目录、选中文件、键盘操作。
// 使用普通 div 而非 ul/li 结构以简化样式控制，同时通过 role="treeitem" 保留无障碍语义。
function FileTreeNodeItem({
  node,
  level,
  onFileSelect,
  selectedFile,
  expandedDirs,
  onToggleDir,
  isDark,
}: {
  node: FileTreeNode;
  level: number;
  onFileSelect?: (file: SkillFileInfo) => void;
  selectedFile?: SkillFileInfo | null;
  expandedDirs: Set<string>;
  onToggleDir: (path: string) => void;
  isDark?: boolean;
}) {
  const isExpanded = expandedDirs.has(node.path);
  const isSelected = selectedFile !== null && node.file !== undefined && selectedFile?.path === node.file?.path;

  const handleClick = () => {
    if (node.isDir) {
      onToggleDir(node.path);
    } else if (node.file && onFileSelect) {
      onFileSelect(node.file);
    }
  };

  // 主题相关颜色
  const hoverBg = isDark ? 'rgba(255,255,255,0.06)' : 'rgba(0,0,0,0.04)';
  const selectedBg = isDark ? 'rgba(59, 130, 246, 0.2)' : 'rgba(37, 99, 235, 0.1)';
  const textColor = isDark ? '#e2e8f0' : '#1e293b';
  const secondaryColor = isDark ? '#94a3b8' : '#64748b';

  return (
    <div>
      <div
        role="treeitem"
        tabIndex={0}
        onClick={handleClick}
        onKeyDown={e => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            handleClick();
          }
        }}
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 6,
          padding: '6px 8px',
          paddingLeft: `${level * 16 + 8}px`,
          cursor: 'pointer',
          borderRadius: 4,
          background: isSelected ? selectedBg : 'transparent',
          borderLeft: isSelected ? '2px solid #3b82f6' : '2px solid transparent',
          transition: 'all 0.15s',
        }}
        onMouseEnter={e => {
          if (!isSelected) {
            e.currentTarget.style.background = hoverBg;
          }
        }}
        onMouseLeave={e => {
          if (!isSelected) {
            e.currentTarget.style.background = 'transparent';
          }
        }}
      >
        {/* 展开/折叠图标 */}
        {node.isDir ? (
          <span style={{ fontSize: 10, color: secondaryColor, width: 12 }}>
            {isExpanded ? '▼' : '▶'}
          </span>
        ) : (
          <span style={{ width: 12 }} />
        )}

        {/* 文件/文件夹图标 */}
        {node.isDir ? (
          isExpanded ? (
            <FolderOpenOutlined style={{ color: '#f59e0b', fontSize: 14 }} />
          ) : (
            <FolderOutlined style={{ color: '#f59e0b', fontSize: 14 }} />
          )
        ) : (
          <FileOutlined style={{ color: getFileColor(node.name, isDark), fontSize: 14 }} />
        )}

        {/* 文件名 */}
        <Text
          style={{
            fontSize: 13,
            flex: 1,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
            color: isSelected ? '#3b82f6' : textColor,
            fontWeight: isSelected ? 500 : 400,
          }}
        >
          {node.name}
        </Text>

        {/* 文件大小 */}
        {node.file && (
          <Text
            style={{ fontSize: 11, flexShrink: 0, color: secondaryColor }}
          >
            {formatSize(node.file.size)}
          </Text>
        )}
      </div>

      {/* 子节点 */}
      {node.isDir && isExpanded && node.children.map(child => (
        <FileTreeNodeItem
          key={child.path}
          node={child}
          level={level + 1}
          onFileSelect={onFileSelect}
          selectedFile={selectedFile}
          expandedDirs={expandedDirs}
          onToggleDir={onToggleDir}
          isDark={isDark}
        />
      ))}
    </div>
  );
}

// SkillFileBrowser：文件树浏览组件。
// 接收文件列表，构建树结构后递归渲染；支持搜索过滤、目录展开/折叠、文件选中高亮。
// 内部状态：searchText（搜索词）、expandedDirs（已展开目录集合）。
export function SkillFileBrowser({ files, loading, onFileSelect, selectedFile, isDark }: SkillFileBrowserProps) {
  const [searchText, setSearchText] = useState('');
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(new Set(['']));

  // 构建文件树
  const fileTree = useMemo(() => buildFileTree(files), [files]);

  // 过滤后的文件树
  const filteredTree = useMemo(() => filterFileTree(fileTree, searchText), [fileTree, searchText]);

  // 切换目录展开/折叠
  const handleToggleDir = (path: string) => {
    setExpandedDirs(prev => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  };

  // 搜索时自动展开所有目录，确保匹配结果可见；
  // 取消搜索时不做特殊处理，已展开的目录由 expandedDirs 状态保持。
  const handleSearch = (value: string) => {
    setSearchText(value);
    if (value) {
      // 搜索时展开所有匹配的目录路径
      const allDirs = new Set<string>(['']);
      files.forEach(file => {
        const parts = file.path.split('/').filter(Boolean);
        parts.forEach((_, index) => {
          allDirs.add(parts.slice(0, index + 1).join('/'));
        });
      });
      setExpandedDirs(allDirs);
    }
  };

  // 主题相关颜色
  const borderColor = isDark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.06)';
  const secondaryColor = isDark ? '#94a3b8' : '#64748b';

  if (loading) {
    return (
      <div style={{ textAlign: 'center', padding: 40 }}>
        <Spin size="large" />
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* 搜索栏 */}
      <div style={{ padding: '12px 12px 8px' }}>
        <Input
          placeholder="搜索文件..."
          prefix={<SearchOutlined style={{ color: secondaryColor }} />}
          value={searchText}
          onChange={e => handleSearch(e.target.value)}
          allowClear
          size="small"
          style={{ borderRadius: 6 }}
        />
      </div>

      {/* 文件统计 */}
      <div style={{
        fontSize: 12,
        color: secondaryColor,
        padding: '0 12px 8px',
        borderBottom: `1px solid ${borderColor}`,
      }}>
        共 {files.length} 个文件
      </div>

      {/* 文件树 */}
      <div style={{
        flex: 1,
        overflow: 'auto',
        padding: '4px 0',
      }}>
        {filteredTree && filteredTree.children.length > 0 ? (
          filteredTree.children.map(node => (
            <FileTreeNodeItem
              key={node.path}
              node={node}
              level={0}
              onFileSelect={onFileSelect}
              selectedFile={selectedFile}
              expandedDirs={expandedDirs}
              onToggleDir={handleToggleDir}
              isDark={isDark}
            />
          ))
        ) : (
          <Empty
            description={searchText ? '没有匹配的文件' : '暂无文件'}
            style={{ padding: '40px 0' }}
          />
        )}
      </div>
    </div>
  );
}
