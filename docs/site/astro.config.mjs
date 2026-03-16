import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  site: "https://zantarix.github.io",
  base: "/cursus",
  integrations: [
    starlight({
      title: "Cursus",
      tagline: "Release management with style",
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/zantarix/cursus",
        },
      ],
      editLink: {
        baseUrl: "https://github.com/zantarix/cursus/edit/main/docs/site/",
      },
      sidebar: [
        {
          label: "Getting Started",
          items: [
            { label: "Installation", slug: "getting-started/installation" },
            { label: "Quick Start", slug: "getting-started/quick-start" },
          ],
        },
        {
          label: "Guides",
          items: [
            {
              label: "Recording Changes",
              slug: "guides/recording-changes",
            },
            {
              label: "Preparing Releases",
              slug: "guides/preparing-releases",
            },
            { label: "Publishing", slug: "guides/publishing" },
            { label: "Utility Commands", slug: "guides/utility-commands" },
            { label: "CI Integration", slug: "guides/ci-integration" },
          ],
        },
        {
          label: "Reference",
          items: [
            { label: "CLI", slug: "reference/cli" },
            { label: "Configuration", slug: "reference/configuration" },
            {
              label: "Changeset Format",
              slug: "reference/changeset-format",
            },
            {
              label: "Package Managers",
              slug: "reference/package-managers",
            },
          ],
        },
        {
          label: "Contributing",
          items: [
            {
              label: "Development Setup",
              slug: "contributing/development-setup",
            },
            { label: "Architecture", slug: "contributing/architecture" },
          ],
        },
        {
          label: "Links",
          items: [
            {
              label: "API Reference (docs.rs)",
              link: "https://docs.rs/cursus/latest/cursus/",
              attrs: { target: "_blank" },
            },
            {
              label: "Architecture Decisions",
              link: "https://github.com/zantarix/cursus/tree/main/docs/adr#readme",
              attrs: { target: "_blank" },
            },
          ],
        },
      ],
    }),
  ],
});
