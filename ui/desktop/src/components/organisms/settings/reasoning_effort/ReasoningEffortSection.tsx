import { useEffect, useState } from 'react';
import { getReasoningEffort, setReasoningEffort } from '@/api';
import {
  ReasoningEffortSelectionItem,
  reasoningEffortOptions,
} from './ReasoningEffortSelectionItem';

export const ReasoningEffortSection = () => {
  const [currentLevel, setCurrentLevel] = useState('medium');
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const fetchLevel = async () => {
      try {
        const { data } = await getReasoningEffort();
        if (data?.level) {
          setCurrentLevel(data.level);
        }
      } catch (error) {
        console.error('Error fetching reasoning effort:', error);
      } finally {
        setLoading(false);
      }
    };
    fetchLevel();
  }, []);

  const handleLevelChange = async (newLevel: string) => {
    const previousLevel = currentLevel;
    setCurrentLevel(newLevel);
    try {
      await setReasoningEffort({ body: { level: newLevel } });
    } catch (error) {
      console.error('Error setting reasoning effort:', error);
      setCurrentLevel(previousLevel);
    }
  };

  if (loading) {
    return (
      <div className="space-y-1">
        {reasoningEffortOptions.map((option) => (
          <div key={option.key} className="h-12 bg-background-muted rounded-lg animate-pulse" />
        ))}
      </div>
    );
  }

  return (
    <div className="space-y-1">
      {reasoningEffortOptions.map((option) => (
        <ReasoningEffortSelectionItem
          key={option.key}
          option={option}
          currentLevel={currentLevel}
          showDescription={true}
          handleLevelChange={handleLevelChange}
        />
      ))}
    </div>
  );
};
