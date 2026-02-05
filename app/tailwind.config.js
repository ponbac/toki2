import { fontFamily as _fontFamily } from "tailwindcss/defaultTheme";
import tailwindcssAnimate from "tailwindcss-animate";
import tailwindcssTypography from "@tailwindcss/typography";

/** @type {import('tailwindcss').Config} */
export const darkMode = "selector";
export const content = [
  "./pages/**/*.{ts,tsx}",
  "./components/**/*.{ts,tsx}",
  "./app/**/*.{ts,tsx}",
  "./src/**/*.{ts,tsx}",
];
export const theme = {
  container: {
    center: true,
    padding: "2rem",
    screens: {
      "2xl": "1400px",
    },
  },
  extend: {
    fontFamily: {
      sans: ["Manrope", "IBM Plex Sans", ..._fontFamily.sans],
      display: ["Manrope", ..._fontFamily.sans],
      mono: ["IBM Plex Mono", ..._fontFamily.mono],
    },
    colors: {
      border: "hsl(var(--border))",
      input: "hsl(var(--input))",
      ring: "hsl(var(--ring))",
      background: "hsl(var(--background))",
      foreground: "hsl(var(--foreground))",
      primary: {
        DEFAULT: "hsl(var(--primary))",
        foreground: "hsl(var(--primary-foreground))",
      },
      secondary: {
        DEFAULT: "hsl(var(--secondary))",
        foreground: "hsl(var(--secondary-foreground))",
      },
      destructive: {
        DEFAULT: "hsl(var(--destructive))",
        foreground: "hsl(var(--destructive-foreground))",
      },
      muted: {
        DEFAULT: "hsl(var(--muted))",
        foreground: "hsl(var(--muted-foreground))",
      },
      accent: {
        DEFAULT: "hsl(var(--accent))",
        foreground: "hsl(var(--accent-foreground))",
      },
      popover: {
        DEFAULT: "hsl(var(--popover))",
        foreground: "hsl(var(--popover-foreground))",
      },
      card: {
        DEFAULT: "hsl(var(--card))",
        foreground: "hsl(var(--card-foreground))",
      },
      glow: "hsl(var(--glow))",
    },
    borderRadius: {
      lg: "var(--radius)",
      md: "calc(var(--radius) - 2px)",
      sm: "calc(var(--radius) - 4px)",
      xl: "calc(var(--radius) + 4px)",
      "2xl": "calc(var(--radius) + 8px)",
    },
    boxShadow: {
      glow: "0 0 20px -5px hsl(var(--glow) / 0.3)",
      "glow-sm": "0 0 10px -3px hsl(var(--glow) / 0.2)",
      "glow-lg": "0 0 40px -10px hsl(var(--glow) / 0.4)",
      elevated:
        "0 1px 2px hsl(var(--foreground) / 0.05), 0 4px 12px hsl(var(--foreground) / 0.08)",
      "elevated-lg":
        "0 2px 4px hsl(var(--foreground) / 0.05), 0 8px 24px hsl(var(--foreground) / 0.1)",
    },
    keyframes: {
      "accordion-down": {
        from: { height: 0 },
        to: { height: "var(--radix-accordion-content-height)" },
      },
      "accordion-up": {
        from: { height: "var(--radix-accordion-content-height)" },
        to: { height: 0 },
      },
      "fade-in": {
        from: { opacity: 0 },
        to: { opacity: 1 },
      },
      "fade-up": {
        from: { opacity: 0, transform: "translateY(10px)" },
        to: { opacity: 1, transform: "translateY(0)" },
      },
      "fade-down": {
        from: { opacity: 0, transform: "translateY(-10px)" },
        to: { opacity: 1, transform: "translateY(0)" },
      },
      "scale-in": {
        from: { opacity: 0, transform: "scale(0.95)" },
        to: { opacity: 1, transform: "scale(1)" },
      },
      "slide-in-right": {
        from: { opacity: 0, transform: "translateX(10px)" },
        to: { opacity: 1, transform: "translateX(0)" },
      },
      shimmer: {
        from: { backgroundPosition: "-200% 0" },
        to: { backgroundPosition: "200% 0" },
      },
      pulse: {
        "0%, 100%": { opacity: 1 },
        "50%": { opacity: 0.5 },
      },
    },
    animation: {
      "accordion-down": "accordion-down 0.2s ease-out",
      "accordion-up": "accordion-up 0.2s ease-out",
      "fade-in": "fade-in 0.3s ease-out",
      "fade-up": "fade-up 0.4s ease-out",
      "fade-down": "fade-down 0.4s ease-out",
      "scale-in": "scale-in 0.2s ease-out",
      "slide-in-right": "slide-in-right 0.3s ease-out",
      shimmer: "shimmer 2s infinite",
      "pulse-slow": "pulse 3s ease-in-out infinite",
    },
    backgroundImage: {
      "gradient-radial": "radial-gradient(var(--tw-gradient-stops))",
      "gradient-warm":
        "linear-gradient(135deg, hsl(var(--primary) / 0.1) 0%, hsl(var(--primary) / 0.05) 100%)",
    },
  },
};
export const plugins = [tailwindcssAnimate, tailwindcssTypography];
