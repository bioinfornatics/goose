export interface ReasoningEffortOption {
  key: string;
  label: string;
  description: string;
}

export const reasoningEffortOptions: ReasoningEffortOption[] = [
  {
    key: 'low',
    label: 'Low',
    description: 'Faster responses with less reasoning — best for simple tasks',
  },
  {
    key: 'medium',
    label: 'Medium',
    description: 'Balanced reasoning depth and speed — good default for most tasks',
  },
  {
    key: 'high',
    label: 'High',
    description: 'Maximum reasoning depth — best for complex analysis and debugging',
  },
];

interface ReasoningEffortSelectionItemProps {
  currentLevel: string;
  option: ReasoningEffortOption;
  showDescription: boolean;
  handleLevelChange: (newLevel: string) => void;
}

export function ReasoningEffortSelectionItem({
  currentLevel,
  option,
  showDescription,
  handleLevelChange,
}: ReasoningEffortSelectionItemProps) {
  const checked = currentLevel === option.key;
  const radioId = `reasoning-effort-${option.key}`;

  return (
    <div className="group text-sm">
      <input
        id={radioId}
        type="radio"
        name="reasoningEffort"
        value={option.key}
        checked={checked}
        onChange={() => handleLevelChange(option.key)}
        className="sr-only"
      />
      <label
        htmlFor={radioId}
        className={`flex cursor-pointer items-center justify-between text-text-default py-2 px-2 ${
          checked ? 'bg-background-muted' : 'bg-background-default hover:bg-background-muted'
        } rounded-lg transition-all`}
      >
        <div className="flex">
          <div>
            <h3 className="text-text-default">{option.label}</h3>
            {showDescription && (
              <p className="text-xs text-text-muted mt-[2px]">{option.description}</p>
            )}
          </div>
        </div>

        <div className="relative flex items-center gap-2">
          <div
            className={`h-4 w-4 rounded-full border transition-all duration-200 ease-in-out ${
              checked
                ? 'border-[6px] border-black dark:border-white bg-white dark:bg-black'
                : 'border-border-default group-hover:border-border-default'
            }`}
          />
        </div>
      </label>
    </div>
  );
}
