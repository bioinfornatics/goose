import { useMemo, useState } from 'react';
import { useLocation } from 'react-router-dom';
import { PageHeader } from '@/components/molecules/design-system/page-header';
import { TabBar } from '@/components/molecules/design-system/tab-bar';
import DatasetsTab from './DatasetsTab';
import EvalOverviewTab from './EvalOverviewTab';
import RoutingInspector from './RoutingInspector';

function ProductionPlaceholder() {
  return (
    <div className="text-center py-16">
      <h3 className="text-lg font-semibold mb-2">🏭 Production Routing Capture</h3>
      <p className="text-muted-foreground">
        Coming soon — auto-capture routing decisions from real sessions.
      </p>
      <p className="text-muted-foreground">Flag misroutes to generate organic training data.</p>
    </div>
  );
}

const TAB_GROUPS = [
  {
    tabs: [
      { id: 'inspector', label: '🔍 Inspector' },
      { id: 'datasets', label: '📋 Datasets & Runs' },
      { id: 'dashboard', label: '📊 Dashboard' },
      { id: 'production', label: '🏭 Production' },
    ],
  },
];

const COMPONENTS: Record<string, React.FC> = {
  inspector: RoutingInspector,
  datasets: DatasetsTab,
  dashboard: EvalOverviewTab,
  production: ProductionPlaceholder,
};

type EvaluateLocationState = {
  tab?: string;
  runId?: string;
};

function getEvaluateState(state: unknown): EvaluateLocationState {
  if (!state || typeof state !== 'object') {
    return {};
  }

  const maybe = state as Partial<EvaluateLocationState>;
  return {
    tab: typeof maybe.tab === 'string' ? maybe.tab : undefined,
    runId: typeof maybe.runId === 'string' ? maybe.runId : undefined,
  };
}

export default function EvaluateView() {
  const location = useLocation();
  const evalState = useMemo(() => getEvaluateState(location.state), [location.state]);
  const [activeTab, setActiveTab] = useState(evalState.tab || 'inspector');
  const ActiveComponent = COMPONENTS[activeTab];

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <div className="flex-shrink-0 px-6 pt-4 pb-0">
        <PageHeader title="Evaluate" />
        <TabBar
          groups={TAB_GROUPS}
          activeTab={activeTab}
          onTabChange={setActiveTab}
          variant="underline"
          className="mt-4"
        />
      </div>
      <div className="flex-1 overflow-y-auto px-6 py-4">
        {ActiveComponent && <ActiveComponent />}
      </div>
    </div>
  );
}
