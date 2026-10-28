const paths = {
  plus: 'M8 3v10M3 8h10',
  close: 'm4 4 8 8M12 4l-8 8',
  dashboard: 'M2 2h4v4H2zM10 2h4v4h-4zM2 10h4v4H2zM10 10h4v4h-4z',
  preset: 'm8 2 6 6-6 6-6-6z',
  view: 'M2 2h12v12H2zM2 6h12',
  grip: 'M5 3h.01M11 3h.01M5 8h.01M11 8h.01M5 13h.01M11 13h.01',
  resize: 'm4 4 8 8M6 12h6V6',
  unavailable: 'M14 8A6 6 0 1 1 2 8a6 6 0 0 1 12 0Z',
} as const;

export function Icon({ name }: { name: keyof typeof paths }) {
  return <svg className="hub-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2"
    strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
    <path d={paths[name]} vectorEffect="non-scaling-stroke" />
  </svg>;
}
