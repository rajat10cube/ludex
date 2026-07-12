/** @type {import('tailwindcss').Config} */
export default {
  darkMode: "class",
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        ink: {
          950: "#0a0f16",
          900: "#0e1620",
          850: "#111c28",
          800: "#16212e",
          700: "#1e2c3c",
          600: "#2a3a4d",
        },
        accent: { DEFAULT: "#4aa3ff", hover: "#68b4ff", soft: "#1b3350" },
        play: "#57cc7a",
      },
      fontFamily: { sans: ["Inter", "Segoe UI", "system-ui", "sans-serif"] },
    },
  },
  plugins: [require("tailwindcss-animate")],
};
