import { describe, it, expect } from 'vitest';
import { bytesToUUIDv4, generateUUID } from './uuid';

// v4 UUID 正则：version 位为 4，variant 位为 8/9/a/b
const V4_REGEX = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

describe('bytesToUUIDv4', () => {
  it('全零字节置位后 version=4 variant=8', () => {
    // 固定输入断言固定输出，验证 version/variant 置位公式正确（无随机性干扰）
    expect(bytesToUUIDv4(new Uint8Array(16))).toBe('00000000-0000-4000-8000-000000000000');
  });

  it('保留随机位、置位后仍符合 v4 格式', () => {
    const bytes = new Uint8Array([
      0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
      0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
    ]);
    // b[6]=0xde → 0x4e（version 位置 4），b[8]=0x11 → 0x91（variant 位置 10），其余位原样保留
    expect(bytesToUUIDv4(bytes)).toBe('12345678-9abc-4ef0-9122-334455667788');
  });

  it('不修改传入的缓冲区（无副作用）', () => {
    const bytes = new Uint8Array(16).fill(0xff);
    bytesToUUIDv4(bytes);
    // 调用后原缓冲区应保持不变（函数内部 slice 复制）
    expect(bytes.every((x) => x === 0xff)).toBe(true);
  });
});

describe('generateUUID', () => {
  it('产出符合 v4 格式', () => {
    expect(generateUUID()).toMatch(V4_REGEX);
  });

  it('两次调用产生不同值', () => {
    // 碰撞概率忽略不计；防回归（如误把 id 写成常量）
    expect(generateUUID()).not.toBe(generateUUID());
  });
});
