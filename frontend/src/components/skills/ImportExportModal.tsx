import { useState, useEffect, useMemo, useCallback } from 'react';
import { Modal, Button, Space, Checkbox, Row, Col, List, Tag, Alert, Upload, message } from 'antd';
import Typography from 'antd/es/typography';
import { ExportOutlined, ImportOutlined, DownloadOutlined, UploadOutlined, SaveOutlined } from '@ant-design/icons';
import * as db from '../../utils/database';
import { normalizeExecutor, type ExportTask } from './helpers';
import type { ExecutorSkills } from '../../types';

const { Text } = Typography;

interface ImportExportModalProps {
  open: boolean;
  mode: 'import' | 'export';
  executor: string;
  data: ExecutorSkills[];
  initialSelectedSkills?: string[];
  onClose: () => void;
}

export function ImportExportModal({ open, mode, executor, data, initialSelectedSkills, onClose }: ImportExportModalProps) {
  const [selectedSkills, setSelectedSkills] = useState<string[]>(initialSelectedSkills || []);
  const [exporting, setExporting] = useState(false);
  const [tasks, setTasks] = useState<ExportTask[]>([]);
  const [importFile, setImportFile] = useState<File | null>(null);
  const [importing, setImporting] = useState(false);

  const executorData = useMemo(() => {
    const normalized = normalizeExecutor(executor);
    let found = data.find(e => e.executor === executor);
    if (!found) {
      found = data.find(e => normalizeExecutor(e.executor) === normalized);
    }
    if (!found && data.length > 0) {
      found = { executor, executor_label: executor, skills: [], skills_dir: '', skills_dir_exists: false };
    }
    return found;
  }, [data, executor]);
  const skills = executorData?.skills || [];

  useEffect(() => {
    if (open) {
      setSelectedSkills(initialSelectedSkills || []);
      setTasks([]);
    }
  }, [open, initialSelectedSkills]);

  const revokeTasksBlobUrls = useCallback((taskList: ExportTask[]) => {
    taskList.forEach(t => { if (t.blobUrl) URL.revokeObjectURL(t.blobUrl); });
  }, []);

  useEffect(() => {
    if (!open) {
      setTasks(prev => {
        revokeTasksBlobUrls(prev);
        return [];
      });
      setSelectedSkills([]);
    }
  }, [open, revokeTasksBlobUrls]);

  const handleExport = async () => {
    if (selectedSkills.length === 0) {
      message.warning('请选择要导出的 Skills');
      return;
    }
    setExporting(true);
    setTasks(prev => {
      revokeTasksBlobUrls(prev);
      return prev;
    });
    const newTasks: ExportTask[] = selectedSkills.map(s => ({
      id: `${Date.now()}-${s}`,
      executor,
      skillName: s,
      status: 'pending',
      progress: 0,
    }));
    setTasks(newTasks);

    let successCount = 0;
    let failCount = 0;

    for (const task of newTasks) {
      setTasks(prev => prev.map(t =>
        t.id === task.id ? { ...t, status: 'exporting' } : t
      ));

      try {
        const blob = await db.exportSkill(task.executor, task.skillName);
        const blobUrl = URL.createObjectURL(blob);

        setTasks(prev => prev.map(t =>
          t.id === task.id ? { ...t, status: 'completed', progress: 100, blobUrl } : t
        ));

        const a = document.createElement('a');
        a.href = blobUrl;
        a.download = `${task.skillName}.zip`;
        a.click();
        successCount++;
      } catch (err: any) {
        setTasks(prev => prev.map(t =>
          t.id === task.id ? { ...t, status: 'failed', error: err.message } : t
        ));
        failCount++;
      }
    }
    setExporting(false);
    if (failCount === 0) {
      message.success(`成功导出 ${successCount} 个 Skills`);
    } else if (successCount === 0) {
      message.error(`导出失败，共 ${failCount} 个 Skills`);
    } else {
      message.info(`导出完成: ${successCount} 个成功, ${failCount} 个失败`);
    }
  };

  const handleImport = async () => {
    if (!importFile) {
      message.warning('请选择要导入的文件');
      return;
    }
    setImporting(true);
    try {
      const result = await db.importSkill(executor, importFile);
      message.success(`导入成功: ${result.skill_name}，共 ${result.imported_files} 个文件`);
      setImportFile(null);
      onClose();
    } catch {
      message.error('导入失败');
    } finally {
      setImporting(false);
    }
  };

  const completedCount = tasks.filter(t => t.status === 'completed').length;

  return (
    <Modal
      title={
        <Space>
          {mode === 'export' ? <ExportOutlined /> : <ImportOutlined />}
          <span>{mode === 'export' ? '导出 Skills' : '导入 Skills'}</span>
        </Space>
      }
      open={open}
      onCancel={onClose}
      width={600}
      footer={
        mode === 'export' ? (
          <Space>
            <Button onClick={onClose}>取消</Button>
            <Button
              type="primary"
              icon={<DownloadOutlined />}
              onClick={handleExport}
              loading={exporting}
              disabled={selectedSkills.length === 0}
            >
              导出 ({selectedSkills.length})
            </Button>
          </Space>
        ) : (
          <Space>
            <Button onClick={onClose}>取消</Button>
            <Button
              type="primary"
              icon={<UploadOutlined />}
              onClick={handleImport}
              loading={importing}
              disabled={!importFile}
            >
              导入
            </Button>
          </Space>
        )
      }
    >
      {mode === 'export' ? (
        <div>
          <Alert
            message="导出说明"
            description="导出的文件为 .zip 压缩包格式，包含 SKILL.md 和所有相关文件。导出后可导入到其他支持 Skills 的应用。"
            type="info"
            showIcon
            style={{ marginBottom: 16 }}
          />

          {tasks.length > 0 ? (
            <List
              size="small"
              dataSource={tasks}
              renderItem={task => (
                <List.Item>
                  <Space style={{ width: '100%' }}>
                    <Text>{task.skillName}</Text>
                    <Tag color={
                      task.status === 'completed' ? 'success' :
                      task.status === 'failed' ? 'error' :
                      task.status === 'exporting' ? 'processing' : 'default'
                    }>
                      {task.status === 'completed' ? '完成' :
                       task.status === 'failed' ? '失败' :
                       task.status === 'exporting' ? `${task.progress}%` : '等待'}
                    </Tag>
                    {task.status === 'completed' && task.blobUrl && (
                      <Button type="link" size="small" icon={<SaveOutlined />} onClick={() => {
                        const a = document.createElement('a');
                        a.href = task.blobUrl!;
                        a.download = `${task.skillName}.zip`;
                        a.click();
                      }}>
                        保存
                      </Button>
                    )}
                  </Space>
                </List.Item>
              )}
            />
          ) : (
            <div style={{ marginBottom: 16 }}>
              <Checkbox.Group
                value={selectedSkills}
                onChange={v => setSelectedSkills(v as string[])}
                style={{ width: '100%' }}
              >
                <Row gutter={[8, 8]}>
                  {skills.length > 0 ? (
                    skills.map(skill => (
                      <Col span={12} key={skill.name}>
                        <Checkbox value={skill.name}>
                          <Text ellipsis style={{ maxWidth: 200 }}>{skill.name}</Text>
                        </Checkbox>
                      </Col>
                    ))
                  ) : (
                    <Col span={24}>
                      <Text type="secondary">该执行器暂无 Skills</Text>
                    </Col>
                  )}
                </Row>
              </Checkbox.Group>
            </div>
          )}

          {tasks.length > 0 && completedCount === tasks.length && (
            <Alert
              message={`成功导出 ${completedCount} 个 Skills`}
              type="success"
              showIcon
              style={{ marginTop: 16 }}
            />
          )}
        </div>
      ) : (
        <div>
          <Alert
            message="导入说明"
            description="支持导入 .zip 压缩包格式的 Skills。导入时可根据目标应用自动处理目录层级。"
            type="info"
            showIcon
            style={{ marginBottom: 16 }}
          />
          <Upload.Dragger
            accept=".zip"
            beforeUpload={(file) => {
              setImportFile(file);
              return false;
            }}
          >
            <p className="ant-upload-drag-icon">
              <UploadOutlined style={{ fontSize: 48, color: '#7C3AED' }} />
            </p>
            <p className="ant-upload-text">点击或拖拽上传 Skills 压缩包</p>
            <p className="ant-upload-hint">支持 .zip 格式</p>
          </Upload.Dragger>
        </div>
      )}
    </Modal>
  );
}
