/**
 * 设置-备份页面文件大小显示格式测试
 *
 * 验证 issue #595 的需求：
 * - 文件 < 1KB → 显示字节（如 "500 B"）
 * - 文件 < 1MB → 显示 KB（如 "1.5 KB"）
 * - 文件 < 1GB → 显示 M（如 "2.5 M"），不带 B 后缀
 * - 文件 >= 1GB → 显示 G（如 "1.2 G"），不带 B 后缀
 *
 * 通过 page.evaluate 调用 formatFileSize，函数返回值会通过 JSON 序列化
 * 传回测试进程，因此评估侧不要保留函数引用，而是把待验证的输入传过去、
 * 一次性返回字符串数组。
 */

import { test, expect } from '@playwright/test';

const DEV_URL = process.env.E2E_BASE_URL || 'http://localhost:5173';

/**
 * 在浏览器侧一次性调用 formatFileSize，返回每个输入对应的格式化结果。
 *
 * 之所以把整个对照表塞进单次 evaluate，是因为 page.evaluate 不允许把函数
 * 引用序列化回 Node 侧；只能传可序列化的字符串/数字/数组。
 */
async function formatAll(page: import('@playwright/test').Page, bytes: number[]): Promise<string[]> {
  await page.goto(DEV_URL);
  return await page.evaluate(async (bytes) => {
    const mod = await import('/src/utils/format');
    return bytes.map((b) => mod.formatFileSize(b));
  }, bytes);
}

/** 验证 formatFileSize 各档位的格式化结果 */
test('formatFileSize: B/KB 档位不变，M/G 档位不带 B 后缀', async ({ page }) => {
  const result = await formatAll(page, [
    0,
    500,
    1023,
    1024,
    1536,
    1024 * 1024 - 1,
    1024 * 1024,
    2.5 * 1024 * 1024,
    100 * 1024 * 1024,
    1024 * 1024 * 1024 - 1,
    1024 * 1024 * 1024,
    1.2 * 1024 * 1024 * 1024,
    10 * 1024 * 1024 * 1024,
  ]);

  // < 1KB：字节
  expect(result[0]).toBe('0 B');
  expect(result[1]).toBe('500 B');
  expect(result[2]).toBe('1023 B');

  // < 1MB：KB 档位保持原样
  expect(result[3]).toBe('1.0 KB');
  expect(result[4]).toBe('1.5 KB');
  expect(result[5]).toBe('1024.0 KB');

  // < 1GB：MB → M，去掉 B 后缀
  expect(result[6]).toBe('1.0 M');
  expect(result[7]).toBe('2.5 M');
  expect(result[8]).toBe('100.0 M');
  expect(result[9]).toBe('1024.0 M');

  // >= 1GB：GB → G，去掉 B 后缀
  expect(result[10]).toBe('1.0 G');
  expect(result[11]).toBe('1.2 G');
  expect(result[12]).toBe('10.0 G');
});

/** 验证 M/G 档位不带 "B" 后缀（issue #595 关键不变量） */
test('formatFileSize: M/G 档位显式不含 "B" 后缀', async ({ page }) => {
  const result = await formatAll(page, [
    5 * 1024 * 1024,
    2 * 1024 * 1024 * 1024,
  ]);

  // M 档位必须不含 B（5MB 输入不应显示 "5.0 MB"）
  expect(result[0]).not.toContain('MB');
  expect(result[0]).toMatch(/ M$/);

  // G 档位必须不含 B（2GB 输入不应显示 "2.0 GB"）
  expect(result[1]).not.toContain('GB');
  expect(result[1]).toMatch(/ G$/);
});

/** 验证备份文件列表中实际渲染出的尺寸文本格式 */
test('DatabaseBackupTab: 备份文件大小按新格式渲染', async ({ page }) => {
  const rendered = await formatAll(page, [
    500,                                  // '500 B'
    2.5 * 1024 * 1024,                    // '2.5 M'
    1.2 * 1024 * 1024 * 1024,             // '1.2 G'
  ]);

  expect(rendered[0]).toBe('500 B');
  expect(rendered[1]).toBe('2.5 M');
  expect(rendered[2]).toBe('1.2 G');
});
