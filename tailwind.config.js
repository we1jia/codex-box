/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  darkMode: "class",
  theme: {
    container: {
      center: true,
      padding: "2rem",
    },
    extend: {
      fontFamily: {
        serif: [
          // 中文衬线优先(思源宋体),英文衬线次之,系统衬线兜底
          "Noto Serif SC",
          "Source Han Serif SC",
          "STSong",
          "Songti SC",
          "serif",
        ],
        sans: [
          // 英文优先 Inter Variable(可变字体,100-900 全字重),
          // 中文走思源黑体 SC,最后系统字体兜底。
          // 浏览器会按字符 unicode-range 自动选择字体,无需手动切换。
          "Inter Variable",
          "Inter",
          "Noto Sans SC",
          "PingFang SC",
          "Hiragino Sans GB",
          "Microsoft YaHei",
          "system-ui",
          "sans-serif",
        ],
        mono: [
          "JetBrains Mono Variable",
          "JetBrains Mono",
          "SF Mono",
          "Menlo",
          "monospace",
        ],
      },
      letterSpacing: {
        // 中英文字距 token 用 :root CSS 变量承载(避免 Tailwind IntelliSense
        // 把带 -cn/-en 后缀的 key 误判为拼写错误,见 ADR-0001)。
        // CSS 用 var(--letter-spacing-xxx) 引用。
      },
      lineHeight: {
        // 同上,中文偏松 / 英文偏紧的行高用 :root CSS 变量承载。
      },
      colors: {
        // 状态色
        status: {
          ok: "#34C759",
          warn: "#FF9F0A",
          fail: "#FF3B30",
        },
        // 文字
        ink: {
          900: "#1C1C1E",
          700: "#3A3A3C",
          500: "#6B6B70",
          400: "#9A9A9F",
          300: "#C7C7CC",
        },
        // 背景
        bg: {
          base: "#F5F5F7",
          deep: "#EAEAEC",
          card: "rgba(255,255,255,0.72)",
        },
      },
      borderRadius: {
        sm: "8px",
        md: "12px",
        lg: "16px",
      },
      boxShadow: {
        card: "0 1px 2px rgba(0,0,0,0.04), 0 8px 24px rgba(0,0,0,0.06)",
        "card-hover":
          "0 2px 4px rgba(0,0,0,0.06), 0 12px 32px rgba(0,0,0,0.10)",
        sticker: "0 2px 4px rgba(0,0,0,0.06), 0 12px 32px rgba(0,0,0,0.10)",
      },
      backdropBlur: {
        glass: "20px",
      },
      // 全局间距 / 宽度 token（与 :root CSS 变量对齐）
      spacing: {
        "titlebar-h": "var(--titlebar-h)",
        "titlebar-l": "var(--titlebar-l)",
        "window-pad": "var(--window-pad)",
        "sidebar-gap": "var(--sidebar-gap)",
        "content-gap": "var(--content-gap)",
        "sidebar-content-gap": "var(--sidebar-content-gap)",
        "content-top": "var(--content-top)",
        "content-right": "var(--content-right)",
        "content-bottom": "var(--content-bottom)",
        "gap-page": "var(--gap-page)",
        sidebar: "var(--sidebar-w)",
        "sidebar-c": "var(--sidebar-w-c)",
      },
      width: {
        sidebar: "var(--sidebar-w)",
        "sidebar-c": "var(--sidebar-w-c)",
      },
      padding: {
        "titlebar-l": "var(--titlebar-l)",
      },
      pl: {
        "titlebar-l": "var(--titlebar-l)",
      },
    },
  },
  plugins: [],
};
