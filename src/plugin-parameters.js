const SAME = 1e-6;

function holderOf(space, { list, slot }) {
  if (list === null) return space;
  if (list === 'source') return space.source;
  return space[list]?.find(item => item.slot === slot);
}

function read(target, path) {
  let value = target;
  for (const key of path) {
    if (value === undefined || value === null) return undefined;
    value = typeof key === 'object' ? (target.reference ? target.offset : target.position)?.[key.place] : value[key];
  }
  return value;
}

function write(target, path, value) {
  let at = target;
  for (const key of path.slice(0, -1)) {
    if (at[key] === undefined || at[key] === null) at[key] = {};
    at = at[key];
  }
  const last = path[path.length - 1];
  if (typeof last === 'object') {
    const list = target.reference ? target.offset : target.position;
    if (Array.isArray(list)) list[last.place] = value;
  } else if (typeof last === 'number') {
    if (Array.isArray(at)) at[last] = value;
  } else {
    at[last] = value;
  }
}

export function plainOf(space, spec) {
  const target = holderOf(space, spec.holder);
  if (!target) return undefined;
  const value = read(target, spec.path);
  switch (spec.kind) {
    case 'number': return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
    case 'choice': { const i = spec.choices.indexOf(value); return i < 0 ? undefined : i; }
    case 'toggle': return typeof value === 'boolean' ? Number(value) : undefined;
    case 'panAuto': return value === null ? 1 : typeof value === 'number' ? 0 : undefined;
    default: return undefined;
  }
}

export function putPlain(space, spec, plain) {
  const target = holderOf(space, spec.holder);
  if (!target) return;
  switch (spec.kind) {
    case 'number': write(target, spec.path, plain); break;
    case 'choice': write(target, spec.path, spec.choices[Math.max(0, Math.round(plain))] ?? spec.choices[0]); break;
    case 'toggle': write(target, spec.path, plain > 0.5); break;
    case 'panAuto': if (plain > 0.5) write(target, spec.path, null); break;
  }
}

export function createParameterSync(send) {
  let specs = [];
  const host = new Map();
  let open = false;
  return {
    get ids() { return specs.map(s => s.id); },
    start(list, values) {
      specs = list;
      host.clear();
      for (const [id, value] of Object.entries(values)) host.set(id, value);
    },
    push(space, commit) {
      for (const spec of specs) {
        const value = plainOf(space, spec);
        if (value === undefined) continue;
        const known = host.get(spec.id);
        if (known !== undefined && Math.abs(value - known) <= SAME * Math.max(1, Math.abs(value))) continue;
        host.set(spec.id, value);
        send({ type: 'set', id: spec.id, value });
        open = true;
      }
      if (commit && open) {
        send({ type: 'end' });
        open = false;
      }
    },
    adopt(space) {
      for (const spec of specs) {
        const value = plainOf(space, spec);
        if (value !== undefined) host.set(spec.id, value);
      }
    },
    take(space, values) {
      for (const spec of specs) {
        if (!(spec.id in values)) continue;
        host.set(spec.id, values[spec.id]);
        putPlain(space, spec, values[spec.id]);
      }
      for (const spec of specs) {
        if (spec.kind === 'panAuto' && host.get(spec.id) > 0.5) putPlain(space, spec, 1);
      }
    }
  };
}
