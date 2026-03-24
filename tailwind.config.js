/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./src/**/*.{js,ts,svelte}",
    "./index.html",
  ],
  theme: {
    extend: {
      colors: {
        neon: {
          cyan: '#00f5ff',
          purple: '#bf00ff',
          pink: '#ff006e',
          green: '#00ff88',
          orange: '#ff8800',
        }
      }
    },
  },
  plugins: [],
}
