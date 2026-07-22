import { useState } from 'react';
import { Button, Popconfirm, Input, ColorPicker, List, Empty, message } from 'antd';
import { DeleteOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import type { Tag } from '@/types';

export function TagsPanel({ tags, dispatch }: { tags: Tag[]; dispatch: any }) {
  const [tagName, setTagName] = useState('');
  const [tagColor, setTagColor] = useState('#0891b2');
  const [tagCreating, setTagCreating] = useState(false);

  const handleCreateTag = async () => {
    if (tagCreating) return;
    const name = tagName.trim();
    if (!name) {
      message.error('请输入标签名称');
      return;
    }
    setTagCreating(true);
    try {
      const newTag = await db.createTag(name, tagColor);
      dispatch({ type: 'ADD_TAG', payload: newTag });
      message.success('标签创建成功');
      setTagName('');
      setTagColor('#0891b2');
    } catch (err: any) {
      message.error('创建失败: ' + (err?.message || String(err)));
    } finally {
      setTagCreating(false);
    }
  };

  const handleDeleteTag = async (tagId: number) => {
    try {
      await db.deleteTag(tagId);
      dispatch({ type: 'DELETE_TAG', payload: tagId });
      message.success('标签已删除');
    } catch (err: any) {
      message.error('删除失败: ' + (err?.message || String(err)));
    }
  };

  return (
    <div style={{ width: '100%' }}>
      <div style={{ marginBottom: 12, fontWeight: 600 }}>创建新标签</div>
      <div style={{ marginBottom: 24, display: 'flex', gap: 12, flexDirection: 'column' }}>
        <Input
          value={tagName}
          onChange={(e) => setTagName(e.target.value)}
          placeholder="输入标签名称"
          onPressEnter={handleCreateTag}
        />
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <ColorPicker
            value={tagColor}
            onChange={(_, hex) => setTagColor(hex)}
            showText
          />
          <Button
            type="primary"
            loading={tagCreating}
            onClick={handleCreateTag}
          >
            创建标签
          </Button>
        </div>
      </div>

      <div style={{ marginBottom: 12, fontWeight: 600 }}>现有标签</div>
      {tags.length === 0 ? (
        <Empty description="暂无标签" image={Empty.PRESENTED_IMAGE_SIMPLE} />
      ) : (
        <List
          dataSource={tags}
          renderItem={(tag) => (
            <List.Item
              style={{
                padding: '10px 12px',
                background: 'var(--color-bg)',
                borderRadius: 6,
                marginBottom: 8,
                border: '1px solid var(--color-border-light)',
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: 10, flex: 1 }}>
                <span
                  style={{
                    width: 16,
                    height: 16,
                    borderRadius: '50%',
                    backgroundColor: tag.color,
                    flexShrink: 0,
                  }}
                />
                <span style={{ fontSize: 14, fontWeight: 500 }}>{tag.name}</span>
              </div>
              <Popconfirm
                title="删除标签"
                description={`确定要删除标签 "${tag.name}" 吗？`}
                onConfirm={() => handleDeleteTag(tag.id)}
              >
                <Button type="text" danger icon={<DeleteOutlined />} size="small" />
              </Popconfirm>
            </List.Item>
          )}
        />
      )}
    </div>
  );
}
