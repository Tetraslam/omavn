import defaultMdxComponents from "fumadocs-ui/mdx";
import type { MDXComponents } from "mdx/types";
import { Clip, Shot } from "@/components/media";

export function getMDXComponents(components?: MDXComponents) {
  return {
    ...defaultMdxComponents,
    Clip,
    Shot,
    ...components,
  } satisfies MDXComponents;
}

export const useMDXComponents = getMDXComponents;

declare global {
  type MDXProvidedComponents = ReturnType<typeof getMDXComponents>;
}
