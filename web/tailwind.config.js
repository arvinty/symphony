/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // Linear-ish dark palette
        bg: "#08090a",
        surface: "#101113",
        elevated: "#16181c",
        border: "#1f2024",
        muted: "#62646a",
        text: "#e6e7e9",
        subtle: "#a3a5ab",
        accent: "#5e6ad2",
        accentSoft: "#7170ff",
        urgent: "#eb5757",
        warn: "#f2c94c",
        ok: "#5fb286",
      },
      fontFamily: {
        sans: ["Inter", "ui-sans-serif", "system-ui", "-apple-system", "Segoe UI", "Roboto"],
        mono: ["JetBrains Mono", "ui-monospace", "SFMono-Regular", "Menlo"],
      },
      boxShadow: {
        soft: "0 1px 0 0 rgba(255,255,255,0.04) inset, 0 1px 2px rgba(0,0,0,0.3)",
        popover: "0 8px 24px rgba(0,0,0,0.5), 0 2px 6px rgba(0,0,0,0.4)",
      },
      fontSize: {
        "2xs": "10px",
      },
    },
  },
  plugins: [],
};
