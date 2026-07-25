// 工艺模板 Tab
// 展示系统工艺与用户工艺的合并列表，支持"复制到用户层"操作。
// 数据从 GET /api/bundled/processes 获取，用户层工艺标记为 is_system=false。

import { useState, useEffect, useCallback } from 'react';
import {
  App,
  Button,
  Empty,
  Modal,
  Space,
  Spin,
  Table,
  Tag,
  Typography,
} from 'antd';
import {
  ReloadOutlined,
  CopyOutlined,
  EyeOutlined,
} from '@ant-design/icons';
import { bundledApi, type ProcessTemplate, type ProcessTemplateDetail } from '@/api/bundled';

const { Text, Paragraph } = Typography;

/**
 * 工艺模板 Tab
 *
 * 在"更多设置 → 模板管理 → 工艺模板"中渲染。
 * 表格列出所有工艺（系统 + 用户），来源列区分系统/用户。
 * 系统工艺行有"复制到用户层"按钮，把系统工艺 YAML 复制到 ~/.ntd/processes/，
 * 之后对它的修改不会被 bundled 同步覆盖。
 */
export function ProcessTemplatesTab({ refreshTick }: { refreshTick?: number }) {
  const { message } = App.useApp();
  const [processes, setProcesses] = useState<ProcessTemplate[]>([]);
  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState<ProcessTemplateDetail | null>(null);
  const [detailOpen, setDetailOpen] = useState(false);
  const [copying, setCopying] = useState<string | null>(null);

  // 加载工艺列表：refreshTick 变化时父组件已触发同步，重新拉取最新数据。
  const loadProcesses = useCallback(async () => {
    setLoading(true);
    try {
      const list = await bundledApi.getProcesses();
      // 按来源分组：用户工艺优先在前，便于用户看到自己的自定义。
      list.sort((a, b) => {
        if (a.is_system !== b.is_system) {
          return a.is_system ? 1 : -1;
        }
        return a.name.localeCompare(b.name);
      });
      setProcesses(list);
    } catch (e: any) {
      message.error('加载工艺列表失败: ' + (e?.message || e));
    } finally {
      setLoading(false);
    }
  }, [message]);

  useEffect(() => {
    loadProcesses();
  }, [loadProcesses, refreshTick]);

  // 复制系统工艺到用户层。点击后调用 copyProcessToUser，成功后刷新列表，
  // 对应工艺的来源从"系统"变为"用户"。
  const handleCopyToUser = async (record: ProcessTemplate) => {
    setCopying(record.name);
    try {
      const result = await bundledApi.copyProcessToUser(record.name);
      message.success(`已复制到用户层：${result.user_source_path}`);
      await loadProcesses();
    } catch (e: any) {
      message.error('复制失败: ' + (e?.message || e));
    } finally {
      setCopying(null);
    }
  };

  // 查看工艺详情：弹出 Modal 展示 YAML 定义。
  const handleView = async (record: ProcessTemplate) => {
    try {
      const detail = await bundledApi.getProcess(record.name);
      setDetail(detail);
      setDetailOpen(true);
    } catch (e: any) {
      message.error('查看详情失败: ' + (e?.message || e));
    }
  };

  const columns = [
    {
      title: '名称',
      dataIndex: 'name',
      key: 'name',
      render: (name: string, record: ProcessTemplate) => (
        <Space direction="vertical" size={0}>
          <Text strong>{record.display_name || name}</Text>
          <Text type="secondary" style={{ fontSize: 12 }}>{name}</Text>
        </Space>
      ),
    },
    {
      title: '分类',
      dataIndex: 'category',
      key: 'category',
      width: 100,
      render: (v: string) => v || '-',
    },
    {
      title: '复杂度',
      dataIndex: 'complexity',
      key: 'complexity',
      width: 100,
      render: (v: string) => {
        const colorMap: Record<string, string> = {
          light: 'green',
          standard: 'blue',
          complex: 'orange',
        };
        return <Tag color={colorMap[v] || 'default'}>{v || '-'}</Tag>;
      },
    },
    {
      title: '版本',
      dataIndex: 'version',
      key: 'version',
      width: 80,
      render: (v: string) => v || '-',
    },
    {
      title: '来源',
      dataIndex: 'is_system',
      key: 'is_system',
      width: 90,
      // 系统工艺来自 bundled 同步，会被 git reset --hard 覆盖；
      // 用户工艺存放在 ~/.ntd/processes/，不会被同步覆盖。
      render: (isSystem: boolean) => isSystem
        ? <Tag color="blue">系统</Tag>
        : <Tag color="green">用户</Tag>,
    },
    {
      title: '操作',
      key: 'actions',
      width: 220,
      render: (_: any, record: ProcessTemplate) => (
        <Space>
          <Button
            type="text"
            size="small"
            icon={<EyeOutlined />}
            onClick={() => handleView(record)}
          >
            查看
          </Button>
          {record.is_system && (
            <Button
              type="text"
              size="small"
              icon={<CopyOutlined />}
              loading={copying === record.name}
              onClick={() => handleCopyToUser(record)}
            >
              复制到用户层
            </Button>
          )}
        </Space>
      ),
    },
  ];

  return (
    <div className="process-templates-tab">
      <Space style={{ marginBottom: 16 }}>
        <Button
          icon={<ReloadOutlined />}
          loading={loading}
          onClick={loadProcesses}
        >
          刷新列表
        </Button>
        <Text type="secondary">
          共 {processes.length} 个工艺
        </Text>
      </Space>

      <Spin spinning={loading}>
        {processes.length === 0 ? (
          <Empty description="暂无工艺模板，请先同步远程仓库">
            <Text type="secondary">
              远程仓库的 processes/ 目录将同步到 ~/.ntd/bundled/processes/
            </Text>
          </Empty>
        ) : (
          <Table
            dataSource={processes}
            columns={columns}
            rowKey="id"
            size="small"
            scroll={{ x: 'max-content' }}
            pagination={{ pageSize: 20 }}
          />
        )}
      </Spin>

      <Modal
        title={detail ? `工艺详情：${detail.display_name || detail.name}` : '工艺详情'}
        open={detailOpen}
        onCancel={() => setDetailOpen(false)}
        footer={null}
        width={800}
      >
        {detail && (
          <Space direction="vertical" style={{ width: '100%' }} size="middle">
            <Paragraph type="secondary" style={{ marginBottom: 0 }}>
              {detail.description}
            </Paragraph>
            <div>
              <Text type="secondary">分类：</Text>
              <Tag>{detail.category}</Tag>
              <Text type="secondary" style={{ marginLeft: 8 }}>复杂度：</Text>
              <Tag>{detail.complexity}</Tag>
              <Text type="secondary" style={{ marginLeft: 8 }}>版本：</Text>
              <Text>{detail.version}</Text>
            </div>
            <Text type="secondary">YAML 定义：</Text>
            <pre
              style={{
                background: '#f5f5f5',
                padding: 12,
                borderRadius: 4,
                maxHeight: 400,
                overflow: 'auto',
                fontSize: 12,
              }}
            >
              {detail.definition}
            </pre>
          </Space>
        )}
      </Modal>
    </div>
  );
}
