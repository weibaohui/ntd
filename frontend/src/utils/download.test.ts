import { describe, it, expect, vi, afterEach } from 'vitest';
import { backupTimestamp, downloadByUrl, downloadBlob } from './download';

// downloadByUrl/downloadBlob 操作真实 DOM 的 <a> 元素：
// 用 appendChild spy 捕获创建的锚点，断言 href/download 赋值与即插即拔的完整生命周期。
describe('downloadByUrl', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('test_downloadByUrl_创建a标签设置属性即插即拔', () => {
    const clickSpy = vi
      .spyOn(HTMLAnchorElement.prototype, 'click')
      // 阻止 jsdom 真实导航（jsdom 对点击跳转只报 not implemented，静默掉保持输出干净）
      .mockImplementation(() => {});
    const appendSpy = vi.spyOn(document.body, 'appendChild');
    const removeSpy = vi.spyOn(document.body, 'removeChild');

    downloadByUrl('/api/v1/backup/todo/file?filename=a.zip', 'a.zip');

    // 元素被插入 body，且属性赋值发生在点击前
    expect(appendSpy).toHaveBeenCalledTimes(1);
    const anchor = appendSpy.mock.calls[0][0] as HTMLAnchorElement;
    expect(anchor.tagName).toBe('A');
    expect(anchor.getAttribute('href')).toBe('/api/v1/backup/todo/file?filename=a.zip');
    expect(anchor.download).toBe('a.zip');
    expect(clickSpy).toHaveBeenCalledTimes(1);
    // 用完即拔，DOM 不残留
    expect(removeSpy).toHaveBeenCalledWith(anchor);
  });
});

describe('downloadBlob', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('test_downloadBlob_objectURL下载后revoke回收', () => {
    // jsdom 未实现 URL.createObjectURL/revokeObjectURL，stub 出可控实现
    const revokeSpy = vi.fn();
    vi.stubGlobal('URL', {
      ...URL,
      createObjectURL: vi.fn().mockReturnValue('blob:mock-url'),
      revokeObjectURL: revokeSpy,
    });
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});

    downloadBlob(new Blob(['x']), 'f.zip');
    expect(URL.createObjectURL).toHaveBeenCalledTimes(1);
    expect(revokeSpy).toHaveBeenCalledWith('blob:mock-url');
  });

  it('test_downloadBlob_抛错时finally仍回收objectURL', () => {
    const revokeSpy = vi.fn();
    vi.stubGlobal('URL', {
      ...URL,
      createObjectURL: vi.fn().mockReturnValue('blob:mock-url'),
      revokeObjectURL: revokeSpy,
    });
    // click 抛错模拟下载触发失败
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {
      throw new Error('click failed');
    });

    expect(() => downloadBlob(new Blob(['x']), 'f.zip')).toThrow('click failed');
    expect(revokeSpy).toHaveBeenCalledWith('blob:mock-url');
  });
});

describe('backupTimestamp', () => {
  it('test_backupTimestamp_输出文件名安全秒级时间戳', () => {
    // 固定时刻：2026-08-11T13:45:06.789Z；冒号与点须替换为 - 才能进 Windows 文件名
    const fixed = new Date('2026-08-11T13:45:06.789Z');
    expect(backupTimestamp(fixed)).toBe('2026-08-11T13-45-06');
  });
});
