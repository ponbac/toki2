import defaultTheme from "tailwindcss/defaultTheme";
import tailwindcssAnimate from "tailwindcss-animate";
import tailwindcssTypography from "@tailwindcss/typography";

const theme = {
  extend: {
    fontFamily: {
      sans: ["Manrope", "IBM Plex Sans", ...defaultTheme.fontFamily.sans],
      display: ["Manrope", ...defaultTheme.fontFamily.sans],
      mono: ["IBM Plex Mono", ...defaultTheme.fontFamily.mono],
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
      elevated:
        "0 1px 2px hsl(var(--foreground) / 0.05), 0 4px 12px hsl(var(--foreground) / 0.08)",
      "elevated-lg":
        "0 2px 4px hsl(var(--foreground) / 0.05), 0 8px 24px hsl(var(--foreground) / 0.1)",
      "elevated-xl":
        "0 4px 8px hsl(var(--foreground) / 0.06), 0 12px 36px hsl(var(--foreground) / 0.12)",
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
      pulse: {
        "0%, 100%": { opacity: 1 },
        "50%": { opacity: 0.5 },
      },
    },
    animation: {
      "accordion-down": "accordion-down 0.2s ease-out",
      "accordion-up": "accordion-up 0.2s ease-out",
      "fade-in": "fade-in 0.3s ease-out",
      "pulse-slow": "pulse 3s ease-in-out infinite",
    },
  },
};
/** Preserve the application design tokens and animation plugins in Tailwind 4. */
export default {
  theme,
  plugins: [tailwindcssAnimate, tailwindcssTypography],
};
