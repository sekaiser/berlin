---
id: theme-components
slug: component-specimens
title: Component specimens
author: [Example Author]
date: 2026-09-01
lastmod: 2026-09-02
description: Synthetic text for checking the notebook's typography and components.
tags: [design, systems]
---
<div class="frontmatter">
<div class="abstract"><p>This is a component specimen, not a personal article. It places ordinary prose, technical material, and navigation within the same reading surface.</p><p>A second introductory paragraph checks that emphasis comes from spacing and a quiet rule rather than a heavier voice.</p></div>
<div class="toc"><div class="heading">Contents</div><ul><li><a href="#palette">Palette</a></li><li><a href="#prose">Prose and tables</a></li><li><a href="#code">Code and disclosures</a></li></ul></div>
</div>

## Palette {#palette}

These swatches use the actual theme tokens. Values are documented in the theme's token file, not repeated as decorative sample colors.

<ul class="theme-swatches">
<li style="--swatch: var(--surface)">Ivory surface</li>
<li style="--swatch: var(--surroundings)">Sage surroundings</li>
<li style="--swatch: var(--ink)">Dark ink</li>
<li style="--swatch: var(--accent)">Terracotta accent</li>
<li style="--swatch: var(--moss)">Moss tag</li>
<li style="--swatch: var(--sky)">Sky tag</li>
<li style="--swatch: var(--apricot)">Apricot tag</li>
<li style="--swatch: var(--lavender)">Lavender tag</li>
</ul>

## Prose and tables {#prose}

An ordinary paragraph should remain comfortable beside a wide technical example.
It can include **strong emphasis**, *a change of voice*, `inline code`, and a
[document link](id:theme-guide). <span class="underline">Org-style underlining</span>
remains supported without a utility stylesheet.

> An observation belongs near the example that prompted it. This quotation is
> synthetic sample text, not attributed advice.

| Element | Responsibility |
| --- | --- |
| Surface | Carry the reading material |
| Margin | Support navigation |
| Tags | Connect related entries |

- Lists must wrap without widening the page.
- Links and buttons must remain reachable from the keyboard.
- A deliberately long identifier such as `a_descriptive_identifier_that_should_wrap_in_prose_but_remain_exact_when_copied` checks narrow screens.

## Code and disclosures {#code}

The listing is rendered by Berlin, so its copy, wrap, and line-link controls are
the same ones used by real articles.

```rust
fn summarize(values: &[u32]) -> (usize, u32) {
    let count = values.len();
    let total = values.iter().sum();
    (count, total)
}

fn main() {
    let values = [3, 5, 8];
    let (count, total) = summarize(&values);
    println!("count={count}, total={total}");
}
```

<details><summary>Inspect the sample result</summary><p>For the displayed input, the count is 3 and the total is 16. This is an explanatory example, not a claim that the publisher executed it.</p></details>

The [Reading page](../feed.html) exercises real search and multi-tag controls,
including their selected and empty-result states. The [Search page](../search.html)
uses the generated index rather than hard-coded specimen results.
