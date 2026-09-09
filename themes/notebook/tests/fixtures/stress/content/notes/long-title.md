---
id: theme-stress
slug: long-title
title: Understanding how a deliberately long engineering notebook title behaves when the available space is narrow and the reader enlarges the text
date: 2026-09-04
author: [Example Author]
description: Synthetic boundary cases for the theme, not a personal article.
tags: [design, systems, parsing, rendering, publishing, performance, reliability, accessibility, architecture, testing, rust, markdown, diagrams, typography, navigation, collections, references, workflows, local-first, configuration]
---

This entry exercises a long title and twenty tags. Every tag must remain readable
and reachable without widening the document or covering the title.

## Wide working material

This URL is intentionally long: <https://example.com/a_deliberately_long_unbroken_path_for_checking_how_normal_article_links_wrap_without_widening_the_document_surface>.

| Measurement | West | East | North | South | Central | Offshore |
| --- | --- | --- | --- | --- | --- | --- |
| `end_to_end_processing_latency_percentile_ninety_nine_milliseconds` | 125 | 130 | 128 | 140 | 121 | 150 |
| `observed_request_throughput_per_second_during_the_test_window` | 8200 | 8100 | 8350 | 7900 | 8500 | 7200 |

The table may scroll internally, but its containing page must not.

<figure class="notebook-result notebook-result--forest" aria-labelledby="wide-result-title">
  <figcaption id="wide-result-title"><span class="result-title">Wide result data</span><span class="result-description">Invented values for checking internal scrolling, not measured performance.</span></figcaption>
  <div class="result-table" role="region" aria-label="Wide synthetic results" tabindex="0">
    <table>
      <caption class="result-accessible-caption">Synthetic values across eight columns</caption>
      <thead><tr><th scope="col">Case</th><th scope="col" class="result-number">First</th><th scope="col" class="result-number">Second</th><th scope="col" class="result-number">Third</th><th scope="col" class="result-number">Fourth</th><th scope="col" class="result-number">Fifth</th><th scope="col" class="result-number">Sixth</th><th scope="col" class="result-number">Seventh</th></tr></thead>
      <tbody><tr><th scope="row">A deliberately long case name</th><td class="result-number">123456.78</td><td class="result-number">123456.78</td><td class="result-number">123456.78</td><td class="result-number">123456.78</td><td class="result-number">123456.78</td><td class="result-number">123456.78</td><td class="result-number">0</td></tr></tbody>
    </table>
  </div>
</figure>

Return to the [component specimens](id:theme-components#code) to inspect code
wrapping and inline explanations.
