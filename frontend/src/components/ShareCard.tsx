import { useState } from 'react';
import { Card, Button, App } from 'antd';
import { ShareAltOutlined, CopyOutlined, CheckOutlined } from '@ant-design/icons';

const getSharePrompt = () => {
  if (typeof window === 'undefined') {
    return `请在我的电脑上全局安装 ntd (Nothing Todo) 这个工具，执行命令：
npm install -g @weibaohui/nothing-todo
安装完成后运行 ntd 启动服务，然后打开浏览器访问 http://localhost:8088`;
  }
  return `请在我的电脑上全局安装 ntd (Nothing Todo) 这个工具，执行命令：
npm install -g @weibaohui/nothing-todo
安装完成后运行 ntd 启动服务，然后打开浏览器访问 ${window.location.origin}`;
};

export function ShareCard() {
  const { message } = App.useApp();
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(getSharePrompt());
      setCopied(true);
      message.success('安装提示词已复制到剪贴板');
      setTimeout(() => setCopied(false), 2000);
    } catch {
      message.error('复制失败，请手动复制');
    }
  };

  return (
    <Card
      title={
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <ShareAltOutlined />
          <span>分享给朋友</span>
        </div>
      }
      className="dashboard-card"
      style={{ borderRadius: 12 }}
      bodyStyle={{ padding: '16px 20px' }}
    >
      <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        <div style={{ fontSize: 13, color: 'var(--color-text-secondary)' }}>
          将下方提示词复制发给 AI 助手，即可自动完成安装：
        </div>
        <div
          style={{
            background: 'var(--color-fill-quaternary)',
            borderRadius: 8,
            padding: '12px 16px',
            fontFamily: 'monospace',
            fontSize: 13,
            lineHeight: 1.8,
            whiteSpace: 'pre-wrap',
            position: 'relative',
          }}
        >
          {getSharePrompt()}
        </div>
        <Button
          type="primary"
          icon={copied ? <CheckOutlined /> : <CopyOutlined />}
          onClick={handleCopy}
          style={{ alignSelf: 'flex-end' }}
        >
          {copied ? '已复制' : '复制提示词'}
        </Button>
      </div>
    </Card>
  );
}
