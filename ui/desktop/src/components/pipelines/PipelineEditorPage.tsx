import { useState, useEffect, useCallback } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { ArrowLeft, Download, FileJson, Save } from 'lucide-react';
import { ReactFlowProvider } from '@xyflow/react';
import { Button } from '../ui/button';
import { getPipeline, updatePipeline, createPipeline } from '../../api';
import type { Pipeline, PipelineNode as ApiNode, PipelineEdge as ApiEdge } from '../../api/types.gen';
import { PipelineEditorCanvas } from './PipelineEditorCanvas';
import { NodePalette } from './NodePalette';
import { TemplateGallery } from './TemplateGallery';
import { pipelineToYaml, pipelineToJson } from './serialization';
import type { PipelineTemplate } from './types';

type SidebarTab = 'nodes' | 'templates';

export function PipelineEditorPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [pipeline, setPipeline] = useState<Pipeline | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sidebarTab, setSidebarTab] = useState<SidebarTab>('nodes');

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

  const handleTemplateSelect = useCallback(async (template: PipelineTemplate) => {
    const { nodes, edges } = template.buildNodes();

    // Convert template nodes/edges to API types
    const apiNodes: ApiNode[] = nodes.map((n) => ({
      id: n.id,
      kind: n.kind,
      label: n.label,
      config: n.config,
      position: n.position ?? null,
      condition: null,
    }));

    const apiEdges: ApiEdge[] = edges.map((e) => ({
      source: e.source,
      target: e.target,
      label: e.label ?? null,
      condition: null,
    }));

    if (pipeline && id) {
      // Apply template to existing pipeline
      const updated: Pipeline = {
        ...pipeline,
        nodes: apiNodes,
        edges: apiEdges,
      };
      setPipeline(updated);
    } else {
      // Create new pipeline from template
      try {
        const res = await createPipeline({
          body: {
            pipeline: {
              name: template.name,
              description: template.description,
              nodes: apiNodes,
              edges: apiEdges,
            },
          },
        });
        if (res.data?.id) {
          navigate(`/pipelines/${res.data.id}`);
        }
      } catch (err) {
        console.error('Failed to create pipeline from template', err);
      }
    }
    setSidebarTab('nodes');
  }, [pipeline, id, navigate]);

  const handleExportYaml = useCallback(() => {
    if (!pipeline) return;
    const yaml = pipelineToYaml(pipeline);
    const blob = new Blob([yaml], { type: 'text/yaml' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${pipeline.name.replace(/\s+/g, '-').toLowerCase()}.yaml`;
    a.click();
    URL.revokeObjectURL(url);
  }, [pipeline]);

  const handleExportJson = useCallback(() => {
    if (!pipeline) return;
    const json = pipelineToJson(pipeline);
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${pipeline.name.replace(/\s+/g, '-').toLowerCase()}.json`;
    a.click();
    URL.revokeObjectURL(url);
  }, [pipeline]);

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
        <div className="flex items-center gap-1">
          <Button variant="ghost" size="sm" onClick={handleExportYaml} title="Export YAML">
            <Download className="size-4" />
            YAML
          </Button>
          <Button variant="ghost" size="sm" onClick={handleExportJson} title="Export JSON">
            <FileJson className="size-4" />
            JSON
          </Button>
          <Button size="sm" disabled={saving} onClick={() => handleSave(pipeline)}>
            <Save className="size-4" />
            {saving ? 'Saving...' : 'Save'}
          </Button>
        </div>
      </div>

      {/* Editor */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar with tabs */}
        <div className="w-56 border-r border-borderSubtle flex flex-col bg-bgApp">
          {/* Tab buttons */}
          <div className="flex border-b border-borderSubtle">
            <button
              type="button"
              onClick={() => setSidebarTab('nodes')}
              className={`flex-1 px-3 py-2 text-xs font-medium transition-colors ${
                sidebarTab === 'nodes'
                  ? 'text-blue-600 border-b-2 border-blue-600'
                  : 'text-gray-500 hover:text-gray-700'
              }`}
            >
              Nodes
            </button>
            <button
              type="button"
              onClick={() => setSidebarTab('templates')}
              className={`flex-1 px-3 py-2 text-xs font-medium transition-colors ${
                sidebarTab === 'templates'
                  ? 'text-blue-600 border-b-2 border-blue-600'
                  : 'text-gray-500 hover:text-gray-700'
              }`}
            >
              Templates
            </button>
          </div>

          {/* Tab content */}
          <div className="flex-1 overflow-y-auto p-2">
            {sidebarTab === 'nodes' ? (
              <NodePalette />
            ) : (
              <TemplateGallery onSelect={handleTemplateSelect} />
            )}
          </div>
        </div>

        <ReactFlowProvider>
          <PipelineEditorCanvas pipeline={pipeline} onSave={handleSave} />
        </ReactFlowProvider>
      </div>
    </div>
  );
}
