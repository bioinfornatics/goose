import { createContext, useContext, useState, useCallback, ReactNode } from 'react';

interface ReasoningDetail {
  title: string;
  content: string;
  messageId?: string;
}

interface ReasoningDetailContextType {
  detail: ReasoningDetail | null;
  isOpen: boolean;
  openDetail: (detail: ReasoningDetail) => void;
  closeDetail: () => void;
  toggleDetail: (detail: ReasoningDetail) => void;
}

const ReasoningDetailContext = createContext<ReasoningDetailContextType | null>(null);

export function useReasoningDetail() {
  const context = useContext(ReasoningDetailContext);
  if (!context) {
    throw new Error('useReasoningDetail must be used within a ReasoningDetailProvider');
  }
  return context;
}

export function ReasoningDetailProvider({ children }: { children: ReactNode }) {
  const [detail, setDetail] = useState<ReasoningDetail | null>(null);
  const [isOpen, setIsOpen] = useState(false);

  const openDetail = useCallback((newDetail: ReasoningDetail) => {
    setDetail(newDetail);
    setIsOpen(true);
  }, []);

  const closeDetail = useCallback(() => {
    setIsOpen(false);
    setTimeout(() => setDetail(null), 300);
  }, []);

  const toggleDetail = useCallback(
    (newDetail: ReasoningDetail) => {
      if (isOpen && detail?.messageId === newDetail.messageId) {
        closeDetail();
      } else {
        openDetail(newDetail);
      }
    },
    [isOpen, detail?.messageId, openDetail, closeDetail]
  );

  return (
    <ReasoningDetailContext.Provider value={{ detail, isOpen, openDetail, closeDetail, toggleDetail }}>
      {children}
    </ReasoningDetailContext.Provider>
  );
}
