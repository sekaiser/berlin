import { defineConfig, presetUno } from "unocss";

export default defineConfig({
  rules: [
    ["white-space-normal", { "white-space": "normal" }],
    ["white-space-nowrap", { "white-space": "nowrap" }],
    ["color-default", { color: "var(--color-fg-default)" }],
    ["transition-ease", {
      transition:
        "color 0.25s ease 0s, border-color 0.25s ease 0s, background-color 0.25s ease 0s, box-shadow 0.25s ease 0s",
    }],
    ["transition-ease-simple", {
      transition: "color 0.25s ease 0s",
    }],

    ["transition-ease-slow", {
      transition: "opacity 250ms ease 100ms",
    }],
    ["scrollbar-w-none", { "scrollbar-width": "none" }],
    ["snap-x-mandatory", { "scroll-snap-type": "x mandatory" }],
    ["bg-gradient", {
      "background": "linear-gradient(rgba(0, 0, 0, 0), rgba(0, 0, 0, 0.6))",
    }],
    ["bg-gradient-hero", {
      "background":
        "linear-gradient(rgba(35, 55, 59, .0), rgba(35, 55, 59, 0.4))",
    }],

    [
      "btn-hover",
      {
        "background-color": "var(--color-Sec3Dark)",
        "border-color": "var(--color-Sec3Dark)",
        color: "white",
        "text-decoration": "underline",
      },
    ],
  ],
  presets: [
    presetUno(),
  ],
  shortcuts: [
    {
      "btn": [
        "relative",
        "font-normal",
        "white-space-nowrap",
        "pl-6",
        "pr-6",
        "inline-flex",
        "h-12",
        "cursor-pointer",
        "border",
        "border-1",
        "border-rounded-8",
        "border-solid",
        "color-default",
        "items-center",
        "justify-center",
        "text-center",
        "transition-ease",
        "hover:btn-hover",
      ].join(" "),
    },
  ],
});
