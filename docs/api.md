---
layout: default
title: API Documentation
---

<div class="card" markdown="1">
## Edgewit API

<div id="redoc-container"></div>
<script src="https://cdn.redoc.ly/redoc/latest/bundles/redoc.standalone.js"></script>
<script>
    Redoc.init(
        "{{ site.baseurl }}/openapi.json",
        {
            theme: {
                colors: {
                    primary: {
                        main: "#ff5e00",
                    },
                },
            },
        },
        document.getElementById("redoc-container"),
    );
</script>
</div>
