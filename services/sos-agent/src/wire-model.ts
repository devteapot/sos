const MAX_EXPERIENCE_ID_BYTES = 128;
const MAX_NAME_BYTES = 64;
const MAX_EXPORTS = 16;
const MAX_DEPENDENCIES = 16;
const MAX_DERIVATION_PARENTS = 8;
const MAX_SCHEMA_DEPTH = 8;
const MAX_SCHEMA_FIELDS = 64;
const MAX_SCHEMA_LIST_ITEMS = 256;
const MAX_BOUNDARY_VALUE_BYTES = 16 * 1024;
const MAX_GRAPH_DEPTH = 4;
const MAX_GRAPH_INSTANCES = 8;
const MAX_GRAPH_SCENE_NODES = 8192;
const MAX_PACKAGE_METADATA_BYTES = 256 * 1024;
const MAX_RESOLVED_GRAPH_BYTES = 256 * 1024;

type JsonObject = Record<string, unknown>;

export interface ExperienceWireFixtureV1 {
  fixture_version: 1;
  instance_id: string;
  limits: {
    exports: number;
    dependencies: number;
    boundary_value_bytes: number;
    graph_depth: number;
    graph_instances: number;
    graph_scene_nodes: number;
  };
  package: JsonObject;
  appearance: JsonObject;
  graph: JsonObject;
}

function fail(path: string, message: string): never {
  throw new Error(`${path}: ${message}`);
}

function object(value: unknown, path: string): JsonObject {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    fail(path, "expected an object");
  }
  return value as JsonObject;
}

function exactKeys(
  value: JsonObject,
  path: string,
  required: readonly string[],
  optional: readonly string[] = [],
) {
  const allowed = new Set([...required, ...optional]);
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) fail(path, `unknown field ${JSON.stringify(key)}`);
  }
  for (const key of required) {
    if (!(key in value)) fail(path, `missing field ${JSON.stringify(key)}`);
  }
}

function integer(value: unknown, path: string, minimum = 0, maximum = Number.MAX_SAFE_INTEGER) {
  if (!Number.isSafeInteger(value) || (value as number) < minimum || (value as number) > maximum) {
    fail(path, `expected an integer between ${minimum} and ${maximum}`);
  }
  return value as number;
}

function boolean(value: unknown, path: string) {
  if (typeof value !== "boolean") fail(path, "expected a boolean");
}

function string(value: unknown, path: string, maxBytes: number) {
  if (typeof value !== "string" || Buffer.byteLength(value) > maxBytes) {
    fail(path, `expected a string of at most ${maxBytes} bytes`);
  }
  return value;
}

function identifier(value: unknown, path: string, maxBytes: number) {
  const decoded = string(value, path, maxBytes);
  if (!/^[a-z][a-z0-9._-]*$/.test(decoded)) fail(path, "invalid identifier");
  return decoded;
}

function digest(value: unknown, path: string) {
  const decoded = string(value, path, 64);
  if (!/^[0-9a-f]{64}$/.test(decoded)) fail(path, "expected a lowercase SHA-256 digest");
  return decoded;
}

function array(value: unknown, path: string, maximum: number) {
  if (!Array.isArray(value) || value.length > maximum) {
    fail(path, `expected an array with at most ${maximum} entries`);
  }
  return value;
}

function validateSchema(value: unknown, path: string, depth = 0) {
  if (depth > MAX_SCHEMA_DEPTH) fail(path, "schema is too deep");
  const schema = object(value, path);
  if (typeof schema.type !== "string") fail(`${path}.type`, "expected a string");
  switch (schema.type) {
    case "null":
    case "boolean":
      exactKeys(schema, path, ["type"]);
      return;
    case "integer":
    case "number": {
      exactKeys(schema, path, ["type"], ["minimum", "maximum"]);
      for (const name of ["minimum", "maximum"] as const) {
        const bound = schema[name];
        if (bound !== undefined) {
          if (schema.type === "integer") integer(bound, `${path}.${name}`, -Number.MAX_SAFE_INTEGER);
          else if (typeof bound !== "number" || !Number.isFinite(bound)) {
            fail(`${path}.${name}`, "expected a finite number");
          }
        }
      }
      if (
        typeof schema.minimum === "number" &&
        typeof schema.maximum === "number" &&
        schema.minimum > schema.maximum
      ) {
        fail(path, "minimum exceeds maximum");
      }
      return;
    }
    case "string": {
      exactKeys(schema, path, ["type", "max_bytes"], ["choices"]);
      const maxBytes = integer(schema.max_bytes, `${path}.max_bytes`, 1, MAX_BOUNDARY_VALUE_BYTES);
      const choices = schema.choices === undefined ? [] : array(schema.choices, `${path}.choices`, 256);
      let previous: string | undefined;
      for (const [index, choice] of choices.entries()) {
        const decoded = string(choice, `${path}.choices[${index}]`, maxBytes);
        if (previous !== undefined && utf8Compare(previous, decoded) >= 0) {
          fail(`${path}.choices`, "choices must be unique and bytewise sorted");
        }
        previous = decoded;
      }
      return;
    }
    case "list":
      exactKeys(schema, path, ["type", "max_items", "items"]);
      integer(schema.max_items, `${path}.max_items`, 1, MAX_SCHEMA_LIST_ITEMS);
      validateSchema(schema.items, `${path}.items`, depth + 1);
      return;
    case "record": {
      exactKeys(schema, path, ["type"], ["fields"]);
      const fields = schema.fields === undefined ? {} : object(schema.fields, `${path}.fields`);
      const names = Object.keys(fields);
      if (names.length > MAX_SCHEMA_FIELDS) fail(`${path}.fields`, "too many fields");
      for (const name of names) {
        identifier(name, `${path}.fields.${name}`, MAX_NAME_BYTES);
        const field = object(fields[name], `${path}.fields.${name}`);
        exactKeys(field, `${path}.fields.${name}`, ["value"], ["required"]);
        if (field.required !== undefined) boolean(field.required, `${path}.fields.${name}.required`);
        validateSchema(field.value, `${path}.fields.${name}.value`, depth + 1);
      }
      return;
    }
    default:
      fail(`${path}.type`, "unsupported schema type");
  }
}

function validateContract(value: unknown, path: string) {
  const contract = object(value, path);
  exactKeys(contract, path, ["contract_version"], ["exports"]);
  if (integer(contract.contract_version, `${path}.contract_version`) !== 1) {
    fail(`${path}.contract_version`, "unsupported version");
  }
  const exports = contract.exports === undefined ? {} : object(contract.exports, `${path}.exports`);
  const entries = Object.entries(exports);
  if (entries.length === 0 || entries.length > MAX_EXPORTS) fail(`${path}.exports`, "invalid count");
  for (const [name, rawExport] of entries) {
    identifier(name, `${path}.exports.${name}`, MAX_NAME_BYTES);
    const exported = object(rawExport, `${path}.exports.${name}`);
    exactKeys(
      exported,
      `${path}.exports.${name}`,
      ["properties", "viewport", "appearance_abi"],
      ["events", "accepts_container_appearance"],
    );
    validateSchema(exported.properties, `${path}.exports.${name}.properties`);
    const events = exported.events === undefined ? {} : object(exported.events, `${path}.exports.${name}.events`);
    if (Object.keys(events).length > MAX_SCHEMA_FIELDS) fail(`${path}.exports.${name}.events`, "too many events");
    for (const [event, schema] of Object.entries(events)) {
      identifier(event, `${path}.exports.${name}.events.${event}`, MAX_NAME_BYTES);
      validateSchema(schema, `${path}.exports.${name}.events.${event}`);
    }
    const viewport = object(exported.viewport, `${path}.exports.${name}.viewport`);
    exactKeys(viewport, `${path}.exports.${name}.viewport`, ["min_width", "min_height", "max_width", "max_height"]);
    const minWidth = integer(viewport.min_width, `${path}.exports.${name}.viewport.min_width`, 1);
    const minHeight = integer(viewport.min_height, `${path}.exports.${name}.viewport.min_height`, 1);
    const maxWidth = integer(viewport.max_width, `${path}.exports.${name}.viewport.max_width`, 1);
    const maxHeight = integer(viewport.max_height, `${path}.exports.${name}.viewport.max_height`, 1);
    if (maxWidth < minWidth || maxHeight < minHeight) fail(`${path}.exports.${name}.viewport`, "invalid bounds");
    if (integer(exported.appearance_abi, `${path}.exports.${name}.appearance_abi`) !== 1) {
      fail(`${path}.exports.${name}.appearance_abi`, "unsupported ABI");
    }
    if (exported.accepts_container_appearance !== undefined) {
      boolean(exported.accepts_container_appearance, `${path}.exports.${name}.accepts_container_appearance`);
    }
  }
}

export function validatePackageV4(value: unknown): asserts value is JsonObject {
  const packageValue = object(value, "$package");
  exactKeys(packageValue, "$package", ["format_version", "experience_id", "role", "contract", "derivation"], ["dependencies"]);
  if (integer(packageValue.format_version, "$package.format_version") !== 4) fail("$package.format_version", "unsupported version");
  identifier(packageValue.experience_id, "$package.experience_id", MAX_EXPERIENCE_ID_BYTES);
  if (packageValue.role !== "ordinary" && packageValue.role !== "shell") fail("$package.role", "invalid role");
  validateContract(packageValue.contract, "$package.contract");
  const dependencies = packageValue.dependencies === undefined ? {} : object(packageValue.dependencies, "$package.dependencies");
  if (Object.keys(dependencies).length > MAX_DEPENDENCIES) fail("$package.dependencies", "too many dependencies");
  for (const [alias, rawDependency] of Object.entries(dependencies)) {
    identifier(alias, `$package.dependencies.${alias}`, MAX_NAME_BYTES);
    const dependency = object(rawDependency, `$package.dependencies.${alias}`);
    exactKeys(dependency, `$package.dependencies.${alias}`, ["experience_id", "revision_id", "export_id", "contract_digest", "policy"], ["grant"]);
    identifier(dependency.experience_id, `$package.dependencies.${alias}.experience_id`, MAX_EXPERIENCE_ID_BYTES);
    digest(dependency.revision_id, `$package.dependencies.${alias}.revision_id`);
    identifier(dependency.export_id, `$package.dependencies.${alias}.export_id`, MAX_NAME_BYTES);
    digest(dependency.contract_digest, `$package.dependencies.${alias}.contract_digest`);
    if (dependency.policy !== "locked" && dependency.policy !== "tracked") fail(`$package.dependencies.${alias}.policy`, "invalid policy");
    const grant = dependency.grant === undefined ? {} : object(dependency.grant, `$package.dependencies.${alias}.grant`);
    exactKeys(grant, `$package.dependencies.${alias}.grant`, [], ["properties", "events"]);
    for (const [kind, values] of [["properties", grant.properties], ["events", grant.events]] as const) {
      if (values === undefined) continue;
      const entries = array(values, `$package.dependencies.${alias}.grant.${kind}`, MAX_SCHEMA_FIELDS);
      let previous: string | undefined;
      for (const [index, entry] of entries.entries()) {
        const decoded = identifier(entry, `$package.dependencies.${alias}.grant.${kind}[${index}]`, MAX_NAME_BYTES);
        if (previous !== undefined && utf8Compare(previous, decoded) >= 0) fail(`$package.dependencies.${alias}.grant.${kind}`, "entries must be unique and bytewise sorted");
        previous = decoded;
      }
    }
  }
  const derivation = object(packageValue.derivation, "$package.derivation");
  exactKeys(derivation, "$package.derivation", ["kind"], ["parents", "request_sha256", "rationale"]);
  if (!(["original", "fork", "remix"] as unknown[]).includes(derivation.kind)) fail("$package.derivation.kind", "invalid kind");
  const parents = derivation.parents === undefined ? [] : array(derivation.parents, "$package.derivation.parents", MAX_DERIVATION_PARENTS);
  for (const [index, rawParent] of parents.entries()) {
    const parent = object(rawParent, `$package.derivation.parents[${index}]`);
    exactKeys(parent, `$package.derivation.parents[${index}]`, ["experience_id", "revision_id"]);
    identifier(parent.experience_id, `$package.derivation.parents[${index}].experience_id`, MAX_EXPERIENCE_ID_BYTES);
    digest(parent.revision_id, `$package.derivation.parents[${index}].revision_id`);
  }
  const expectedParents = derivation.kind === "original" ? 0 : derivation.kind === "fork" ? 1 : 2;
  if ((derivation.kind !== "remix" && parents.length !== expectedParents) || (derivation.kind === "remix" && parents.length < 2)) fail("$package.derivation.parents", "count does not match kind");
  if (derivation.kind === "original" && (derivation.request_sha256 !== undefined || derivation.rationale !== undefined)) fail("$package.derivation", "original cannot include request metadata");
  if (derivation.kind !== "original") {
    digest(derivation.request_sha256, "$package.derivation.request_sha256");
    if (string(derivation.rationale, "$package.derivation.rationale", 4096).trim() === "") fail("$package.derivation.rationale", "must not be empty");
  }
}

export function validateAppearanceV1(value: unknown): asserts value is JsonObject {
  const appearance = object(value, "$appearance");
  exactKeys(appearance, "$appearance", ["abi_version", "generation", "scheme", "contrast", "text_scale_milli", "reduce_motion"], ["colors", "spacing", "radii", "typography"]);
  if (integer(appearance.abi_version, "$appearance.abi_version") !== 1) fail("$appearance.abi_version", "unsupported ABI");
  integer(appearance.generation, "$appearance.generation");
  if (appearance.scheme !== "light" && appearance.scheme !== "dark") fail("$appearance.scheme", "invalid scheme");
  if (appearance.contrast !== "standard" && appearance.contrast !== "high") fail("$appearance.contrast", "invalid contrast");
  integer(appearance.text_scale_milli, "$appearance.text_scale_milli", 500, 3000);
  boolean(appearance.reduce_motion, "$appearance.reduce_motion");
  const colors = appearance.colors === undefined ? {} : object(appearance.colors, "$appearance.colors");
  const spacing = appearance.spacing === undefined ? {} : object(appearance.spacing, "$appearance.spacing");
  const radii = appearance.radii === undefined ? {} : object(appearance.radii, "$appearance.radii");
  const typography = appearance.typography === undefined ? {} : object(appearance.typography, "$appearance.typography");
  if (Object.keys(colors).length + Object.keys(spacing).length + Object.keys(radii).length + Object.keys(typography).length > MAX_SCHEMA_FIELDS * 4) fail("$appearance", "too many tokens");
  for (const [name, rawColor] of Object.entries(colors)) {
    identifier(name, `$appearance.colors.${name}`, MAX_NAME_BYTES);
    if (typeof rawColor !== "string" || !/^#[0-9a-fA-F]{8}$/.test(rawColor)) fail(`$appearance.colors.${name}`, "invalid color");
  }
  for (const [kind, tokens] of [["spacing", spacing], ["radii", radii]] as const) {
    for (const [name, amount] of Object.entries(tokens)) {
      identifier(name, `$appearance.${kind}.${name}`, MAX_NAME_BYTES);
      integer(amount, `$appearance.${kind}.${name}`, 0, 65535);
    }
  }
  for (const [name, rawToken] of Object.entries(typography)) {
    identifier(name, `$appearance.typography.${name}`, MAX_NAME_BYTES);
    const token = object(rawToken, `$appearance.typography.${name}`);
    exactKeys(token, `$appearance.typography.${name}`, ["family", "size_milli_points", "weight", "line_height_milli"]);
    if (string(token.family, `$appearance.typography.${name}.family`, 128) === "") fail(`$appearance.typography.${name}.family`, "must not be empty");
    integer(token.size_milli_points, `$appearance.typography.${name}.size_milli_points`, 1000, 512000);
    integer(token.weight, `$appearance.typography.${name}.weight`, 1, 1000);
    integer(token.line_height_milli, `$appearance.typography.${name}.line_height_milli`, 500, 4000);
  }
  if (Buffer.byteLength(canonicalJson(appearance)) > MAX_BOUNDARY_VALUE_BYTES) fail("$appearance", "serialized value is too large");
}

export function validateResolvedGraphV1(value: unknown): asserts value is JsonObject {
  const graph = object(value, "$graph");
  exactKeys(graph, "$graph", ["format_version", "root", "nodes"]);
  if (integer(graph.format_version, "$graph.format_version") !== 1) fail("$graph.format_version", "unsupported version");
  const root = identifier(graph.root, "$graph.root", MAX_EXPERIENCE_ID_BYTES);
  const nodes = object(graph.nodes, "$graph.nodes");
  const entries = Object.entries(nodes);
  if (entries.length === 0 || entries.length > MAX_GRAPH_INSTANCES) fail("$graph.nodes", "invalid count");
  if (!(root in nodes)) fail("$graph.root", "root node is missing");
  const experienceRevisions = new Map<string, string>();
  for (const [id, rawNode] of entries) {
    identifier(id, `$graph.nodes.${id}`, MAX_EXPERIENCE_ID_BYTES);
    const node = object(rawNode, `$graph.nodes.${id}`);
    exactKeys(node, `$graph.nodes.${id}`, ["experience_id", "revision_id", "export_id"], ["parent", "dependency"]);
    identifier(node.experience_id, `$graph.nodes.${id}.experience_id`, MAX_EXPERIENCE_ID_BYTES);
    digest(node.revision_id, `$graph.nodes.${id}.revision_id`);
    identifier(node.export_id, `$graph.nodes.${id}.export_id`, MAX_NAME_BYTES);
    const experienceId = node.experience_id as string;
    const revisionId = node.revision_id as string;
    const existingRevision = experienceRevisions.get(experienceId);
    if (existingRevision !== undefined && existingRevision !== revisionId) {
      fail(`$graph.nodes.${id}`, `experience ${experienceId} appears at more than one revision`);
    }
    experienceRevisions.set(experienceId, revisionId);
    if (id === root) {
      if (node.parent !== undefined || node.dependency !== undefined) fail(`$graph.nodes.${id}`, "root cannot have a parent");
    } else {
      identifier(node.parent, `$graph.nodes.${id}.parent`, MAX_EXPERIENCE_ID_BYTES);
      identifier(node.dependency, `$graph.nodes.${id}.dependency`, MAX_NAME_BYTES);
      if (!(node.parent as string in nodes)) fail(`$graph.nodes.${id}.parent`, "parent is missing");
    }
  }
  for (const [id] of entries) {
    const seen = new Set<string>();
    let cursor = id;
    let depth = 0;
    while (cursor !== root) {
      if (seen.has(cursor)) fail(`$graph.nodes.${id}`, "cycle detected");
      seen.add(cursor);
      const parent: unknown = object(nodes[cursor], `$graph.nodes.${cursor}`).parent;
      if (typeof parent !== "string") fail(`$graph.nodes.${cursor}.parent`, "missing parent");
      cursor = parent;
      depth += 1;
      if (depth > MAX_GRAPH_DEPTH) fail(`$graph.nodes.${id}`, "graph is too deep");
    }
  }
}

function utf8Compare(left: string, right: string) {
  return Buffer.compare(Buffer.from(left), Buffer.from(right));
}

export function canonicalJson(value: unknown): string {
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new Error("canonical JSON cannot encode a non-finite number");
    return JSON.stringify(value);
  }
  if (typeof value === "string") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  const record = object(value, "$canonical");
  return `{${Object.keys(record)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${canonicalJson(record[key])}`)
    .join(",")}}`;
}

function decodeCanonical(text: string, kind: string, limit: number) {
  if (Buffer.byteLength(text) > limit) throw new Error(`${kind} JSON exceeds ${limit} bytes`);
  const value: unknown = JSON.parse(text);
  if (canonicalJson(value) !== text) throw new Error(`${kind} JSON is not canonical`);
  return value;
}

export function decodeCanonicalPackageV4(text: string): JsonObject {
  const value = decodeCanonical(text, "package", MAX_PACKAGE_METADATA_BYTES);
  validatePackageV4(value);
  return value;
}

export function decodeCanonicalResolvedGraphV1(text: string): JsonObject {
  const value = decodeCanonical(text, "resolved graph", MAX_RESOLVED_GRAPH_BYTES);
  validateResolvedGraphV1(value);
  return value;
}

export function decodeExperienceWireFixtureV1(text: string): ExperienceWireFixtureV1 {
  const fixture = object(JSON.parse(text), "$fixture");
  exactKeys(fixture, "$fixture", ["fixture_version", "instance_id", "limits", "package", "appearance", "graph"]);
  if (integer(fixture.fixture_version, "$fixture.fixture_version") !== 1) fail("$fixture.fixture_version", "unsupported version");
  identifier(fixture.instance_id, "$fixture.instance_id", MAX_EXPERIENCE_ID_BYTES);
  const limits = object(fixture.limits, "$fixture.limits");
  exactKeys(limits, "$fixture.limits", ["exports", "dependencies", "boundary_value_bytes", "graph_depth", "graph_instances", "graph_scene_nodes"]);
  const expectedLimits: Record<string, number> = {
    exports: MAX_EXPORTS,
    dependencies: MAX_DEPENDENCIES,
    boundary_value_bytes: MAX_BOUNDARY_VALUE_BYTES,
    graph_depth: MAX_GRAPH_DEPTH,
    graph_instances: MAX_GRAPH_INSTANCES,
    graph_scene_nodes: MAX_GRAPH_SCENE_NODES,
  };
  for (const [name, expected] of Object.entries(expectedLimits)) {
    if (integer(limits[name], `$fixture.limits.${name}`) !== expected) fail(`$fixture.limits.${name}`, `expected ${expected}`);
  }
  validatePackageV4(fixture.package);
  validateAppearanceV1(fixture.appearance);
  validateResolvedGraphV1(fixture.graph);
  return fixture as unknown as ExperienceWireFixtureV1;
}
