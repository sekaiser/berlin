---
id: recovery-design
title: Recovery design
kind: guide
slug: recovery-design
date: 2026-01-02
tags: [distributed-systems, durability]
description: Rebuild the serving view from the completed journal.
---

## Recovery {#recovery}

The [handoff](id:storage-latency) determines which prefix can be replayed.

![A synthetic handoff diagram](/attachments/handoff.svg)
