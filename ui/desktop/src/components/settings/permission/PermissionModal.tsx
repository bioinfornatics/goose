import PermissionRulesModal from './PermissionRulesModal';

interface PermissionModalProps {
  extensionName: string;
  onClose: () => void;
}

export default function PermissionModal({ extensionName, onClose }: PermissionModalProps) {
  return <PermissionRulesModal isOpen onClose={onClose} extensionFilter={extensionName} />;
}
