EDGEWIT INDEX DEFINITION SPECIFICATION

PURPOSE
Index definition files provide deterministic schema configuration for Edgewit indexes.
They ensure reproducible deployments, predictable ingestion behavior, and minimal runtime schema inference. The specification is intentionally small and stable to maintain simplicity and suitability for embedded and edge environments.

DESIGN PRINCIPLES

- simple human readable format
- deterministic schema
- minimal parsing complexity
- reproducible infrastructure deployments
- small runtime overhead
- forward compatible
- compatible with container environments
- no dynamic schema mutation

FILE FORMAT
YAML (preferred for readability)
JSON support optional in future

FILE NAMING
<index-name>.index.yaml

EXAMPLES
logs.index.yaml
metrics.index.yaml
traces.index.yaml

INDEX DEFINITION STRUCTURE

name
unique index name

description (optional)
human readable description

timestamp_field (optional) = default is "timestamp"
explicit field name used for time partitioning and retention routing (required if partitioning enabled)

mode (optional)
schema enforcement mode (strict, drop_unmapped, dynamic)

partition (optional)
time partitioning strategy

retention (optional)
data retention policy

compression (optional)
segment compression algorithm

fields
schema definition for indexed documents

settings (optional)
future configuration area

EXAMPLE INDEX DEFINITION

name: logs

description: application log events

timestamp_field: timestamp

mode: drop_unmapped

partition: daily

retention: 7d

compression: zstd

fields:
timestamp:
type: datetime
indexed: true
fast: true

level:
type: keyword

service:
type: keyword

device_id:
type: keyword

message:
type: text

FIELD TYPES

text
full text indexed field

keyword
exact match string field

datetime
timestamp field

integer
signed integer

float
floating point value

boolean
true or false

bytes
binary data

SCHEMA MODE OPTIONS

strict
reject incoming documents that contain fields not defined in the schema

drop_unmapped
ingest document but silently discard fields not present in the schema

dynamic
automatically infer types for new fields and add them to the index

FIELD PROPERTIES

type
required field type

indexed
whether field participates in search

stored
whether individual field value is stored separately (note: Edgewit typically stores the original JSON document as `_source`, so this is rarely needed unless fast individual retrieval is required without parsing)

fast
optimized column access for aggregations

optional
whether field may be absent

default
default value if missing

PARTITION STRATEGIES

none
single index without partitioning

daily
one partition per day

hourly
one partition per hour

monthly
one partition per month

RETENTION FORMAT

number followed by time unit

units
s seconds
m minutes
h hours
d days
w weeks
M months
Y years

COMPRESSION OPTIONS

none
zstd
lz4

VALIDATION RULES

index name must be unique
field names must be unique
timestamp field required if partitioning enabled
unsupported field types rejected
schema mutation not allowed after index creation
unknown configuration fields ignored for forward compatibility

INDEX DIRECTORY STRUCTURE

/data
/indexes
logs.index.yaml
metrics.index.yaml
/segments
/wal
/metadata

RUNTIME METADATA STORAGE

Each index stores schema metadata internally to allow recovery if definition files are missing.

Example structure

/data/indexes/logs/
definition.yaml
metadata.json
segments/

API COMPATIBILITY

Index definitions may be created using API endpoints.

Example endpoint

PUT /indexes/logs

Body example

{
"fields": {
"timestamp": "datetime",
"service": "keyword",
"message": "text"
}
}

Edgewit converts API definitions into persistent index definition files.

IMPLEMENTATION PLAN

PHASE 1 FILE PARSER (COMPLETED)

goals
implement parser for index definition files

tasks
create index_definition module
define schema structs
implement YAML parser
implement validation logic
load definitions at startup
log configuration errors

deliverables
IndexDefinition struct
FieldDefinition struct
partition and retention enums
validation system

PHASE 2 INDEX REGISTRY (COMPLETED)

goals
central index management

tasks
implement IndexRegistry
register indexes during startup
ensure unique index names
store loaded definitions in memory
provide index lookup API

deliverables
IndexRegistry module
index lookup system
index configuration cache

PHASE 3 SCHEMA INTEGRATION (COMPLETED)

goals
convert index definitions to search engine schema

tasks
map field types to internal indexing types
generate Tantivy schema
configure fast fields for aggregations
initialize index writer

deliverables
schema generation module
index initialization pipeline

PHASE 4 INGESTION VALIDATION (COMPLETED)

goals
validate incoming documents

tasks
validate field types
reject incompatible documents
apply default values
ignore unknown fields optionally
generate error responses

deliverables
ingestion validation layer
schema enforcement

PHASE 5 PARTITION MANAGEMENT (COMPLETED)

goals
support time partitioned indexes

tasks
implement partition resolver
create partition naming convention
route documents to correct partitions
handle partition rollover

deliverables
partition routing module
partition directory structure

PHASE 6 SEGMENT COMPACTION (COMPLETED)

goals
optimize query performance and manage file handles on edge devices

tasks
implement background task to detect tiny segments
merge small segments into larger ones within partitions
manage file handle limits

deliverables
background compaction worker

PHASE 7 RETENTION MANAGEMENT (COMPLETED)

goals
automatically remove expired partitions

tasks
scan partition metadata
calculate expiration time
delete expired segments
run periodic retention worker

deliverables
retention worker
partition expiration logic

PHASE 8 API INDEX MANAGEMENT (CRUD) (COMPLETED)

goals
support full lifecycle management of indexes via API

tasks
implement PUT /indexes/<name> endpoint to create/update indexes
implement DELETE /indexes/<name> endpoint to remove index and wipe data
validate incoming schema on creation/update
persist definition files to disk when created via API
reload index registry dynamically
support configuration to lock/deny dynamic API updates for strict environments

deliverables
index creation, update, and deletion APIs
definition persistence and dynamic state reloading

PHASE 9 OBSERVABILITY (COMPLETED)

goals
visibility into index configuration

tasks
implement GET /indexes endpoint
return index metadata
expose partition status
expose retention configuration

deliverables
index inspection endpoints

PHASE 10 STARTUP RECOVERY (COMPLETED)

goals
safe restart behavior

tasks
rebuild index registry directly from disk (disk YAML is the absolute source of truth)
verify schema consistency against internal metadata
recover partition state
validate segment metadata

deliverables
startup recovery logic

LONG TERM EXTENSIONS

possible future capabilities

index templates
schema versioning
migration tooling
schema evolution policies
cross index queries
multi tenant index namespaces

DESIGN CONSTRAINTS

index definition system must remain under approximately 1000 lines of code
schema parser must remain dependency light
index creation must remain deterministic
runtime memory overhead minimal
startup time minimal

FINAL GOAL

Edgewit index definitions provide deterministic schema configuration suitable for edge deployments while maintaining a small implementation footprint and predictable operational behavior.
