const stroke = body => `<svg class="ui-icon object-glyph" viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round">${body}</svg>`;

const ICONS = Object.freeze({
  plane: stroke('<path d="M5 18 9 6h10l-4 12Z"/>'),
  disc: stroke('<ellipse cx="12" cy="12" rx="5" ry="8"/>'),
  grating: stroke('<path d="M5 18 9 6h10l-4 12Z"/><path d="M8.5 15.5h8M10 11h8M11.3 7h0M7.5 9.5h9" stroke-width="1.1"/><path d="M11 18l3-12M14 18l3-12" stroke-width="1.1"/>'),
  wall: stroke('<path d="M4 4h16v16H4Z"/><circle cx="12" cy="12" r="3.5"/>'),
  box: stroke('<path d="M4 8l8-4 8 4v8l-8 4-8-4Z"/><path d="M4 8l8 4 8-4M12 12v8"/>'),
  corner: stroke('<path d="M12 4v10l-8 4M12 14l8 4"/><path d="M4 6l8-2 8 2" opacity=".4"/>'),
  recess: stroke('<path d="M4 4h16v16H4Z"/><path d="M8 8h8v8H8Z"/><path d="M8 8l2 2h4l2-2M10 10v4h4v-4M8 16l2-2M16 16l-2-2"/>'),
  pipe: stroke('<ellipse cx="6" cy="12" rx="2.5" ry="5"/><path d="M6 7h12M6 17h12"/><path d="M18 7a2.5 5 0 0 1 0 10"/>')
});

export const BUILD_KINDS = Object.freeze([
  ['plane', 'Plane'], ['disc', 'Disc'], ['wall', 'Wall with opening'], ['grating', 'Grating'],
  ['box', 'Box'], ['corner', 'Corner'], ['recess', 'Niche'], ['pipe', 'Pipe']
]);

export const objectIcon = kind => ICONS[kind] ?? ICONS.plane;
