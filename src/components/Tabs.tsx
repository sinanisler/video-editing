export function Tabs(props: { value: string; onChange: (id: string) => void; tabs: [id: string, label: string, tip: string][] }) {
  return (
    <div className="tabs">
      {props.tabs.map(([id, label, tip]) => (
        <button key={id} className={id === props.value ? "act" : ""} data-tip={tip} onClick={() => props.onChange(id)}>
          {label}
        </button>
      ))}
    </div>
  );
}
