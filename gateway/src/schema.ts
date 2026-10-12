// Minimal JSON Schema (draft 2020-12 subset) validator.
//
// Supports exactly the keywords used by contracts/schemas: type (incl. type
// arrays), const, enum, pattern, minLength, maxLength, minimum, maximum,
// minItems, items, required, properties, additionalProperties: false, and a
// lenient date-time format check. Any other keyword makes validation FAIL —
// loudly — so schemas can never drift past what the gateway can enforce.

export interface ValidationIssue {
  path: string;
  message: string;
}

const SUPPORTED_KEYWORDS = new Set([
  "$schema",
  "$id",
  "title",
  "description",
  "type",
  "const",
  "enum",
  "pattern",
  "minLength",
  "maxLength",
  "minimum",
  "maximum",
  "minItems",
  "items",
  "required",
  "properties",
  "additionalProperties",
  "format",
]);

const ISO_DATE_TIME = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$/;
const PATTERN_CACHE = new Map<string, RegExp>();

export function validate(schema: Record<string, unknown>, value: unknown, path = "$"): ValidationIssue[] {
  const issues: ValidationIssue[] = [];
  validateNode(schema, value, path, issues);
  return issues;
}

function fail(path: string, message: string, issues: ValidationIssue[]) {
  issues.push({ path, message });
}

function validateNode(schema: unknown, value: unknown, path: string, issues: ValidationIssue[]) {
  if (typeof schema !== "object" || schema === null) {
    fail(path, "schema must be an object", issues);
    return;
  }
  const s = schema as Record<string, unknown>;

  for (const key of Object.keys(s)) {
    if (!SUPPORTED_KEYWORDS.has(key)) {
      fail(path, `unsupported schema keyword: ${key}`, issues);
    }
  }

  const types = asArray(s.type);
  if (types.length > 0 && !types.includes(typeOf(value))) {
    fail(path, `expected type ${types.join(" | ")}, got ${typeOf(value)}`, issues);
    return;
  }

  if ("const" in s && value !== s.const) {
    fail(path, `expected const ${JSON.stringify(s.const)}`, issues);
  }
  if (Array.isArray(s.enum) && !s.enum.includes(value as never)) {
    fail(path, `value not in enum ${JSON.stringify(s.enum)}`, issues);
  }
  if (typeof value === "string") {
    const str = value;
    if (typeof s.minLength === "number" && str.length < s.minLength) {
      fail(path, `shorter than minLength ${s.minLength}`, issues);
    }
    if (typeof s.maxLength === "number" && str.length > s.maxLength) {
      fail(path, `longer than maxLength ${s.maxLength}`, issues);
    }
    if (typeof s.pattern === "string" && !pattern(String(s.pattern)).test(str)) {
      fail(path, `does not match pattern ${String(s.pattern)}`, issues);
    }
    if (s.format === "date-time" && !ISO_DATE_TIME.test(str)) {
      fail(path, "not an RFC 3339 date-time", issues);
    }
  }
  if (typeof value === "number") {
    if (typeof s.minimum === "number" && value < s.minimum) {
      fail(path, `below minimum ${s.minimum}`, issues);
    }
    if (typeof s.maximum === "number" && value > s.maximum) {
      fail(path, `above maximum ${s.maximum}`, issues);
    }
  }
  if (Array.isArray(value)) {
    if (typeof s.minItems === "number" && value.length < s.minItems) {
      fail(path, `fewer than minItems ${s.minItems}`, issues);
    }
    if (s.items !== undefined) {
      value.forEach((item, i) => validateNode(s.items, item, `${path}[${i}]`, issues));
    }
  }
  if (typeOf(value) === "object") {
    const obj = value as Record<string, unknown>;
    const required = Array.isArray(s.required) ? (s.required as string[]) : [];
    for (const key of required) {
      if (!(key in obj)) fail(`${path}.${key}`, "required field missing", issues);
    }
    const properties = (s.properties ?? {}) as Record<string, unknown>;
    if (s.additionalProperties === false) {
      for (const key of Object.keys(obj)) {
        if (!(key in properties)) fail(`${path}.${key}`, "additional property not allowed", issues);
      }
    }
    for (const [key, subSchema] of Object.entries(properties)) {
      if (key in obj) validateNode(subSchema, obj[key], `${path}.${key}`, issues);
    }
  }
}

function typeOf(value: unknown): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return "array";
  if (Number.isInteger(value)) return "integer";
  if (typeof value === "number") return "number";
  return typeof value;
}

function asArray(t: unknown): string[] {
  if (t === undefined) return [];
  return Array.isArray(t) ? (t as string[]) : [t as string];
}

function pattern(source: string): RegExp {
  let re = PATTERN_CACHE.get(source);
  if (!re) {
    re = new RegExp(source);
    PATTERN_CACHE.set(source, re);
  }
  return re;
}
