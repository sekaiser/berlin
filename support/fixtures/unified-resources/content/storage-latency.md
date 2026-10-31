---
id: storage-latency
title: Storage latency
slug: storage-latency
previous_slugs: [old-storage-latency]
date: 2026-01-01
tags: [system-design, storage]
description: A synthetic example of a completed prefix.
---

## The handoff {#handoff}

Publish the completed position after synchronization. See the
[recovery design](id:recovery-design) and the [logging paper](https://example.test/papers/logging.pdf).[^durability]

```rust {id=example, lineanchors=handoff-line, linenostart=1}
writer.sync_all()?;
publish(completed_position);
```

The [first line](#handoff-line-1) precedes publication.

![A synthetic handoff diagram](/attachments/handoff.svg)

[Download the synthetic record format](/attachments/format.txt).

[^durability]: The completed prefix records the service's acknowledgement boundary.
