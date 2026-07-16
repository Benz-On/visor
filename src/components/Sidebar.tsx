import {
  Activity,
  Bell,
  Bot,
  ChartNoAxesCombined,
  ChevronLeft,
  ChevronRight,
  Cpu,
  History,
  LayoutDashboard,
  Settings,
} from 'lucide-react';

export type ViewId = 'overview' | 'processes' | 'performance' | 'ai' | 'history' | 'alerts' | 'settings';

interface SidebarProps {
  active: ViewId;
  collapsed: boolean;
  onNavigate: (view: ViewId) => void;
  onCollapse: () => void;
}

const primaryItems = [
  { id: 'overview' as const, label: 'Overview', icon: LayoutDashboard },
  { id: 'processes' as const, label: 'Processes', icon: Cpu },
  { id: 'performance' as const, label: 'Performance', icon: ChartNoAxesCombined },
  { id: 'ai' as const, label: 'AI workloads', icon: Bot, badge: '1' },
];

const secondaryItems = [
  { id: 'history' as const, label: 'History', icon: History },
  { id: 'alerts' as const, label: 'Alerts', icon: Bell, badge: '2' },
];

export function Sidebar({ active, collapsed, onNavigate, onCollapse }: SidebarProps) {
  const renderItem = ({ id, label, icon: Icon, badge }: (typeof primaryItems)[number] | (typeof secondaryItems)[number]) => (
    <button
      key={id}
      className={`nav-item ${active === id ? 'nav-item-active' : ''}`}
      onClick={() => onNavigate(id)}
      title={collapsed ? label : undefined}
    >
      <Icon size={19} strokeWidth={1.8} />
      <span>{label}</span>
      {badge && <em>{badge}</em>}
    </button>
  );

  return (
    <aside className={`sidebar ${collapsed ? 'sidebar-collapsed' : ''}`}>
      <div className="brand">
        <div className="brand-mark" aria-hidden="true">
          <Activity size={20} />
        </div>
        <div className="brand-wordmark">
          <strong>VISOR</strong>
          <span>SYSTEM INTELLIGENCE</span>
        </div>
      </div>

      <nav className="sidebar-nav" aria-label="Primary navigation">
        <div className="nav-section">{!collapsed && <p>MONITOR</p>}{primaryItems.map(renderItem)}</div>
        <div className="nav-section">{!collapsed && <p>INSIGHTS</p>}{secondaryItems.map(renderItem)}</div>
      </nav>

      <div className="sidebar-bottom">
        <div className="collector-status">
          <span className="collector-pulse" />
          <div>
            <strong>Live preview</strong>
            <span>900 ms refresh</span>
          </div>
        </div>
        <button
          className={`nav-item ${active === 'settings' ? 'nav-item-active' : ''}`}
          onClick={() => onNavigate('settings')}
          title={collapsed ? 'Settings' : undefined}
        >
          <Settings size={19} strokeWidth={1.8} />
          <span>Settings</span>
        </button>
        <button className="collapse-button" onClick={onCollapse} aria-label="Toggle sidebar">
          {collapsed ? <ChevronRight size={17} /> : <ChevronLeft size={17} />}
        </button>
      </div>
    </aside>
  );
}
