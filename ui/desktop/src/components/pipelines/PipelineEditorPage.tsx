import { useState, useEffect } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { ArrowLeft, Save } from 'lucide-react';
import { ReactFlowProvider } from '@xyflow/react';
import { Button } from '../ui/button';
import { getPipeline, updatePipeline } from '../../api';
import type { Pipeline } from '../../api/types.gen';
import { PipelineEditorCanvas } from './PipelineEditorCanvas';
import { NodePalette } from './NodePalette';

export function PipelineEditorPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [pipeline, setPipeline] = useState<Pipeline | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!id) return;
    const fetchPipeline = async () => {
      setLoading(true);
      try {
        const res = await getPipeline({ path: { id } });
        if (res.data) {
          setPipeline(res.data.pipeline);
        }
      } catch (err) {
        console.error('Failed to load pipeline', err);
        setError('Failed to load pipeline');
      } finally {
        setLoading(false);
      }
    };
    fetchPipeline();
  }, [id]);

  const handleSave = async (updated: Pipeline) => {
    if (!id) return;
    setSaving(true);
    try {
      await updatePipeline({
        path: { id },
        body: { pipeline: updated },
      });
      setPipeline(updated);
    } catch (err) {
      console.error('Failed to save pipeline', err);
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-textSubtle">Loading pipeline...</div>
      </div>
    );
  }

  if (error || !pipeline) {
    return (
      <div className="flex flex-col items-center justify-center gap-4 h-full">
        <div className="text-red-500">{error || 'Pipeline not found'}</div>
        <Button variant="outline" onClick={() => navigate('/pipelines')}>
          Back to Pipelines
        </Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between p-3 border-b border-borderSubtle">
        <div className="flex items-center gap-2">
          <Button variant="ghost" size="sm" shape="round" onClick={() => navigate('/pipelines')}>
            <ArrowLeft className="size-4" />
          </Button>
          <h1 className="text-lg font-semibold truncate">{pipeline.name}</h1>
        </div>
        <Button size="sm" disabled={saving} onClick={() => handleSave(pipeline)}>
          <Save className="size-4" />
          {saving ? 'Saving...' : 'Save'}
        </Button>
      </div>

      {/* Editor */}
      <div className="flex flex-1 overflow-hidden">
        <NodePalette />
        <ReactFlowProvider>
          <PipelineEditorCanvas pipeline={pipeline} onSave={handleSave} />
        </ReactFlowProvider>
      </div>
    </div>
  );
}
