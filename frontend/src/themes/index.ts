import type { ThemeConfig } from 'antd';
import { theme } from 'antd';

// Catppuccin Mocha inspired dark palette + custom cyan accent
const catppuccinMocha = {
  base: '#1e1e2e',
  mantle: '#11111b',
  crust: '#0a0a0f',
  text: '#cdd6f4',
  subtext1: '#a6adc8',
  subtext0: '#6c7086',
  surface0: '#313244',
  surface1: '#45475a',
  surface2: '#585b70',
  overlay0: '#6c7086',
  blue: '#89b4fa',
  lavender: '#b4befe',
  sapphire: '#74c7ec',
  sky: '#89dceb',
  teal: '#94e2d5',
  green: '#a6e3a1',
  yellow: '#f9e2af',
  peach: '#fab387',
  maroon: '#eba0ac',
  red: '#f38ba8',
  mauve: '#cba6f7',
  pink: '#f5c2e7',
  flamingo: '#f2cdcd',
  rosewater: '#f5e0dc',
};

// Custom cyan accent (keeping original primary feel)
const cyanAccent = {
  primary: '#0891b2',
  primaryHover: '#0e7490',
  primaryLight: '#0c4a5e',
  primaryBg: '#0a2e3d',
};

const sharedToken = {
  colorPrimary: cyanAccent.primary,
  colorSuccess: catppuccinMocha.green,
  colorWarning: catppuccinMocha.yellow,
  colorError: catppuccinMocha.red,
  colorInfo: catppuccinMocha.blue,
  borderRadius: 12,
  borderRadiusLG: 16,
  borderRadiusSM: 8,
  fontFamily: "'JetBrains Mono', 'SF Mono', 'Cascadia Code', monospace",
  fontSize: 14,
  controlHeight: 40,
  lineHeight: 1.5,
};

const sharedComponents = {
  Button: {
    borderRadius: 10,
    controlHeight: 40,
    paddingInline: 20,
  },
  Card: {
    borderRadius: 16,
    paddingLG: 24,
  },
  Modal: {
    borderRadiusLG: 16,
    paddingContentHorizontalLG: 24,
  },
  Input: {
    borderRadius: 10,
    paddingInline: 14,
  },
  Select: {
    borderRadius: 10,
  },
  Tag: {
    borderRadius: 6,
  },
  Switch: {
    colorPrimary: cyanAccent.primary,
  },
};

const lightTheme: ThemeConfig = {
  algorithm: theme.defaultAlgorithm,
  token: {
    ...sharedToken,
    colorBgContainer: '#ffffff',
    colorBgLayout: '#f8fafc',
    colorText: '#0f172a',
    colorTextSecondary: '#475569',
    colorBorder: '#e2e8f0',
    colorBorderSecondary: '#f1f5f9',
    boxShadow: '0 4px 12px rgba(0, 0, 0, 0.08)',
    boxShadowSecondary: '0 8px 24px rgba(0, 0, 0, 0.12)',
  },
  components: sharedComponents,
};

const darkTheme: ThemeConfig = {
  algorithm: theme.darkAlgorithm,
  token: {
    ...sharedToken,
    colorBgContainer: catppuccinMocha.base,
    colorBgLayout: catppuccinMocha.mantle,
    colorText: catppuccinMocha.text,
    colorTextSecondary: catppuccinMocha.subtext1,
    colorBorder: catppuccinMocha.surface0,
    colorBorderSecondary: catppuccinMocha.surface1,
    colorPrimaryHover: cyanAccent.primaryHover,
    colorPrimaryBg: cyanAccent.primaryBg,
    boxShadow: '0 4px 12px rgba(0, 0, 0, 0.4)',
    boxShadowSecondary: '0 8px 24px rgba(0, 0, 0, 0.5)',
  },
  components: {
    ...sharedComponents,
    Button: {
      ...sharedComponents.Button,
      colorBgContainer: catppuccinMocha.surface0,
      colorBgElevated: catppuccinMocha.surface0,
      colorText: catppuccinMocha.text,
      colorBorder: catppuccinMocha.surface0,
    },
    Input: {
      ...sharedComponents.Input,
      colorBgContainer: catppuccinMocha.surface0,
      colorText: catppuccinMocha.text,
      colorBorder: catppuccinMocha.surface1,
      activeBorderColor: cyanAccent.primary,
      hoverBorderColor: catppuccinMocha.surface2,
    },
    Select: {
      ...sharedComponents.Select,
      colorBgContainer: catppuccinMocha.surface0,
      colorBgElevated: catppuccinMocha.surface0,
      colorText: catppuccinMocha.text,
      colorBorder: catppuccinMocha.surface1,
      optionSelectedBg: cyanAccent.primaryBg,
    },
    Card: {
      ...sharedComponents.Card,
      colorBgContainer: catppuccinMocha.base,
      colorBorderSecondary: catppuccinMocha.surface0,
    },
    Modal: {
      ...sharedComponents.Modal,
      colorBgContainer: catppuccinMocha.base,
      colorBorderSecondary: catppuccinMocha.surface0,
    },
    Tabs: {
      colorBgContainer: 'transparent',
      colorBorderSecondary: catppuccinMocha.surface0,
      colorText: catppuccinMocha.subtext1,
      colorPrimary: cyanAccent.primary,
      itemSelectedColor: cyanAccent.primary,
      itemHoverColor: catppuccinMocha.text,
    },
    Segmented: {
      colorBgLayout: catppuccinMocha.surface0,
      itemColor: catppuccinMocha.subtext1,
      itemSelectedBg: catppuccinMocha.surface1,
      itemSelectedColor: catppuccinMocha.text,
      itemHoverBg: catppuccinMocha.surface1,
    },
    Table: {
      colorBgContainer: catppuccinMocha.base,
      colorFillSecondary: catppuccinMocha.surface0,
      colorBorderSecondary: catppuccinMocha.surface0,
      headerColor: catppuccinMocha.subtext1,
      colorText: catppuccinMocha.text,
    },
    List: {
      colorBgContainer: catppuccinMocha.base,
      colorBorderSecondary: catppuccinMocha.surface0,
    },
    Popconfirm: {
      colorBgElevated: catppuccinMocha.surface0,
    },
    Dropdown: {
      colorBgElevated: catppuccinMocha.surface0,
    },
    Tooltip: {
      colorBgSpotlight: catppuccinMocha.surface1,
    },
    Message: {
      colorBgElevated: catppuccinMocha.surface0,
    },
    Notification: {
      colorBgElevated: catppuccinMocha.surface0,
    },
    ColorPicker: {
      colorBgElevated: catppuccinMocha.surface0,
      colorBorder: catppuccinMocha.surface1,
      colorPrimary: cyanAccent.primary,
      colorText: catppuccinMocha.text,
    },
  },
};

export type ThemeMode = 'light' | 'dark';

export const themeMap: Record<ThemeMode, ThemeConfig> = {
  light: lightTheme,
  dark: darkTheme,
};
