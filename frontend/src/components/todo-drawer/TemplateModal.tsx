import { useState, useMemo } from 'react';
import { Modal, Input, Tag, Spin, Empty, Card } from 'antd';
import { FileTextOutlined, SearchOutlined } from '@ant-design/icons';
import type { TodoTemplate } from '../../types';

export function TemplateModal({ open, templates, loading, onClose, onSelect }: {
  open: boolean;
  templates: TodoTemplate[];
  loading: boolean;
  onClose: () => void;
  onSelect: (template: TodoTemplate) => void;
}) {
  const [searchText, setSearchText] = useState('');
  const [selectedCategory, setSelectedCategory] = useState<string | null>(null);

  const categories = useMemo(() => {
    const cats = Array.from(new Set(templates.map(t => t.category))).filter(c => c);
    return cats.sort();
  }, [templates]);

  const filteredTemplates = useMemo(() => {
    let result = templates;
    if (selectedCategory) {
      result = result.filter(t => t.category === selectedCategory);
    }
    if (searchText.trim()) {
      const search = searchText.toLowerCase();
      result = result.filter(t =>
        t.title.toLowerCase().includes(search) ||
        (t.prompt?.toLowerCase().includes(search))
      );
    }
    return result;
  }, [templates, selectedCategory, searchText]);

  const handleClose = () => {
    setSearchText('');
    setSelectedCategory(null);
    onClose();
  };

  return (
    <Modal
      title={
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <FileTextOutlined style={{ color: 'var(--color-primary)' }} />
          <span>选择模板</span>
        </div>
      }
      open={open}
      onCancel={handleClose}
      footer={null}
      width={900}
    >
      <div className="template-selector">
        <div className="template-search">
          <Input
            placeholder="搜索模板标题或内容..."
            allowClear
            value={searchText}
            onChange={(e) => setSearchText(e.target.value)}
            style={{ width: '100%' }}
            size="large"
            prefix={<SearchOutlined style={{ color: 'var(--color-text-tertiary)' }} />}
          />
        </div>

        <div className="template-content" style={{ display: 'flex', gap: 16, flex: 1, minHeight: 0, overflow: 'hidden' }}>
          <div className="template-categories" style={{ width: 180, flexShrink: 0, display: 'flex', flexDirection: 'column', gap: 4, borderRight: '1px solid var(--color-border-light)', paddingRight: 16, overflowY: 'auto' }}>
            <div
              className={`template-category-item ${!selectedCategory ? 'active' : ''}`}
              onClick={() => setSelectedCategory(null)}
              style={{ display: 'flex', alignItems: 'center', padding: '10px 12px', borderRadius: 8, cursor: 'pointer', transition: 'all 0.2s ease', color: !selectedCategory ? 'var(--color-primary)' : 'var(--color-text-secondary)', fontWeight: !selectedCategory ? 600 : 400, fontSize: 14 }}
            >
              <span>全部模板</span>
              <Tag style={{ marginLeft: 'auto' }}>{templates.length}</Tag>
            </div>
            {categories.map(category => {
              const count = templates.filter(t => t.category === category).length;
              return (
                <div
                  key={category}
                  className={`template-category-item ${selectedCategory === category ? 'active' : ''}`}
                  onClick={() => setSelectedCategory(category)}
                  style={{ display: 'flex', alignItems: 'center', padding: '10px 12px', borderRadius: 8, cursor: 'pointer', transition: 'all 0.2s ease', color: selectedCategory === category ? 'var(--color-primary)' : 'var(--color-text-secondary)', fontWeight: selectedCategory === category ? 600 : 400, fontSize: 14 }}
                >
                  <span>{category}</span>
                  <Tag style={{ marginLeft: 'auto' }}>{count}</Tag>
                </div>
              );
            })}
          </div>

          <div className="template-list" style={{ flex: 1, overflowY: 'auto', paddingLeft: 16 }}>
            <Spin spinning={loading}>
              {filteredTemplates.length === 0 ? (
                <Empty
                  description={searchText ? "未找到匹配的模板" : "暂无模板，请在设置中添加"}
                  image={Empty.PRESENTED_IMAGE_SIMPLE}
                />
              ) : (
                <div className="template-cards" style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 12 }}>
                  {filteredTemplates.map(template => (
                    <Card
                      key={template.id}
                      size="small"
                      className="template-card"
                      onClick={() => onSelect(template)}
                      hoverable
                      style={{ cursor: 'pointer', transition: 'all 0.2s ease', border: '1px solid var(--color-border-light)' }}
                    >
                      <div style={{ display: 'flex', alignItems: 'center', marginBottom: 8 }}>
                        <span style={{ fontWeight: 600, fontSize: 14, color: 'var(--color-text)' }}>{template.title}</span>
                        {template.is_system && <Tag color="blue" style={{ marginLeft: 8 }}>系统</Tag>}
                      </div>
                      <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', lineHeight: 1.5, maxHeight: 60, overflow: 'hidden', textOverflow: 'ellipsis', display: '-webkit-box', WebkitLineClamp: 3, WebkitBoxOrient: 'vertical', wordBreak: 'break-word' }}>
                        {template.prompt || '(无内容)'}
                      </div>
                      <div style={{ marginTop: 8, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                        <Tag>{template.category}</Tag>
                      </div>
                    </Card>
                  ))}
                </div>
              )}
            </Spin>
          </div>
        </div>
      </div>

      <style>{`
        .template-card:hover {
          border-color: var(--color-primary) !important;
          box-shadow: 0 2px 8px rgba(0, 0, 0, 0.09);
          transform: translateY(-2px);
        }
        @media (max-width: 768px) {
          .template-content {
            flex-direction: column !important;
          }
          .template-categories {
            width: 100% !important;
            flex-direction: row !important;
            flex-wrap: wrap;
            gap: 8px;
            border-right: none !important;
            border-bottom: 1px solid var(--color-border-light);
            padding-right: 0 !important;
            padding-bottom: 16px;
            overflow-x: auto !important;
          }
          .template-list {
            padding-left: 0 !important;
            padding-top: 16px;
          }
          .template-cards {
            grid-template-columns: 1fr !important;
          }
        }
      `}</style>
    </Modal>
  );
}
