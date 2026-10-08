import { useId, useState } from "react";
import { perMillion } from "../ai";
import type { AiModel } from "../ai";

/**
 * Picks a model from the list OpenRouter sent. It is a native datalist, so the box filters as you type and offers the list
 * when you click it. The box shows the chosen model's slug (its id), so the search and the choice are in the same place.
 * Picking a row, or typing a model's exact name or id, chooses it.
 */
export function ModelPicker(props: {
  /** Every model OpenRouter sent, or null while they load. The chosen one is named from this even when the box is filtered. */
  known: AiModel[] | null;
  /** The models the box offers: chat models or speech-to-text models. */
  options: AiModel[];
  chosen: string | null;
  defaultId: string;
  /** True when the list could not be loaded, so the box can say so. */
  failed: boolean;
  tip: string;
  onChoose: (id: string) => void;
}) {
  const { known, options, chosen, defaultId, failed, tip, onChoose } = props;
  const listId = useId();
  // What the user has typed since the box took focus. Null shows the chosen model's name.
  const [typed, setTyped] = useState<string | null>(null);
  const picked = known?.find((m) => m.id === chosen) ?? null;
  // The box shows the model's slug, the id OpenRouter and the app both use. The name is only in the list.
  const shown = typed ?? chosen ?? "";
  const placeholder =
    known === null ? "Loading models from OpenRouter…" : failed ? "The model list could not be loaded" : options.length === 0 ? "No models to choose from" : "Search or pick a model";

  // Typing a model's exact name or id chooses it; the box then shows its slug.
  const change = (text: string) => {
    const match = options.find((m) => m.name === text || m.id === text);
    setTyped(match ? match.id : text);
    if (match && match.id !== chosen) onChoose(match.id);
  };

  return (
    <div className="picker">
      <input
        list={listId}
        value={shown}
        placeholder={placeholder}
        data-tip={tip}
        onFocus={(e) => e.currentTarget.select()}
        onChange={(e) => change(e.target.value)}
        onBlur={() => setTyped(null)}
      />
      <datalist id={listId}>
        {options.map((m) => (
          <option key={m.id} value={m.id} label={m.name} />
        ))}
      </datalist>
      <div className="picked">
        {chosen === null ? (
          <span className="dim">No model chosen</span>
        ) : (
          <>
            <div className="picked-row">
              <span className="dim mono">{chosen}</span>
              {chosen === defaultId ? (
                <span className="badge" data-tip="The model the app starts with">
                  Default
                </span>
              ) : (
                <button className="ghost" data-tip={`Go back to the default, ${defaultId}`} onClick={() => onChoose(defaultId)}>
                  Use default
                </button>
              )}
            </div>
            {picked && <small className="dim">{details(picked)}</small>}
          </>
        )}
      </div>
    </div>
  );
}

/** What the summary says about a model: its context and price, or how a speech-to-text model is billed. */
function details(m: AiModel): string {
  // Speech-to-text is billed by the length of the audio, so a per-token price would mislead.
  if (m.output.includes("transcription")) return "priced by audio length";
  const context = m.context_length ? `${Math.round(m.context_length / 1000)}k context` : "context unknown";
  return `${context} · ${perMillion(m.prompt_price)}`;
}
