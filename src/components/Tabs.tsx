export function Tabs(props: { value: string; onChange: (id: string) => void; tabs: [id: string, label: string][] }) {
  return (
    <div className="tabs">
      {props.tabs.map(([id, label]) => (
        <button key={id} className={id === props.value ? "act" : ""} onClick={() => props.onChange(id)}>
          {label}
        </button>
      ))}
    </div>
  );
}
