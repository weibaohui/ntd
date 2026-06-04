// 获取或生成设备ID
export function getDeviceId(): string {
  let deviceId = localStorage.getItem('ntd_device_id');
  if (!deviceId) {
    deviceId = 'device-' + Math.random().toString(36).substr(2, 9) + '-' + Date.now().toString(36);
    localStorage.setItem('ntd_device_id', deviceId);
  }
  return deviceId;
}

// 获取设备名称
export function getDeviceName(): string {
  return navigator.platform || 'Unknown Device';
}
