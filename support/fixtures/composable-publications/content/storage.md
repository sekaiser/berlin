---
id: storage
title: Storage handoffs
date: 2026-01-01
tags: [system-design, storage]
description: A synthetic example of a **completed prefix**.
---

## The handoff {#handoff}

Publish the position after synchronization. See [queue admission](id:queues)
for the other side of the contract, or the [recovery note](#recovery).[^durability]

```rust {id=example, lineanchors=handoff-line, linenostart=1}
writer.sync_all()?;
publish(completed_position);
```

The [synchronization line](#handoff-line-1) precedes publication.

![A synthetic handoff diagram](/attachments/handoff.svg)

[Download the synthetic record format](/attachments/format.txt).

## Recovery {#recovery}

Replay the completed prefix before serving a freshness-dependent lookup.

[^durability]: A synchronization result is part of a storage protocol, not a universal hardware guarantee.
