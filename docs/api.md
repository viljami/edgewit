---
layout: default
title: API Documentation
---

<div class="card" style="background-color: #ffffff; color: #333333;" markdown="1">
## Edgewit API

<div id="redoc-container"></div>
<script src="https://cdn.redoc.ly/redoc/latest/bundles/redoc.standalone.js"></script>
<script>
    Redoc.init(
        "{{ '/openapi.json' | relative_url }}",
        {
            theme: {
                colors: {
                    primary: {
                        main: "#58a6ff",
                    },
                },
            },
        },
        document.getElementById("redoc-container"),
    );
</script>
</div>
