import {
  Activity,
  Bell,
  Bot,
  ChartNoAxesCombined,
  ChevronLeft,
  ChevronRight,
  Cpu,
  History,
  Leaf,
  LayoutDashboard,
  Settings,
} from 'lucide-react';

export type ViewId = 'overview' | 'processes' | 'performance' | 'ai' | 'energy' | 'history' | 'alerts' | 'settings';

interface SidebarProps {
  active: ViewId;
  collapsed: boolean;
  onNavigate: (view: ViewId) => void;
  onCollapse: () => void;
  connection: 'connecting' | 'live' | 'demo' | 'error';
  activeModelCount: number;
  activeAlertCount: number;
  collectorSource?: 'windows-agent' | 'tauri-native';
}

const primaryItems = [
  { id: 'overview' as const, label: 'Overview', icon: LayoutDashboard },
  { id: 'processes' as const, label: 'Processes', icon: Cpu },
  { id: 'performance' as const, label: 'Performance', icon: ChartNoAxesCombined },
  { id: 'ai' as const, label: 'AI workloads', icon: Bot },
  { id: 'energy' as const, label: 'Energy lens', icon: Leaf, badge: 'LIVE' },
];

const secondaryItems = [
  { id: 'history' as const, label: 'History', icon: History },
  { id: 'alerts' as const, label: 'Alerts', icon: Bell },
];

export function Sidebar({ active, collapsed, onNavigate, onCollapse, connection, activeModelCount, activeAlertCount, collectorSource }: SidebarProps) {
  const renderItem = (item: (typeof primaryItems)[number] | (typeof secondaryItems)[number]) => {
    const { id, label, icon: Icon } = item;
    const staticBadge = 'badge' in item ? item.badge : undefined;
    const badge = id === 'ai' ? (activeModelCount > 0 ? String(activeModelCount) : undefined) : id === 'alerts' ? (activeAlertCount > 0 ? String(activeAlertCount) : undefined) : staticBadge;
    return (
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
  };

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
            <strong>{connection === 'live' ? (collectorSource === 'tauri-native' ? 'Native collector' : 'Development agent') : connection === 'connecting' ? 'Connecting…' : 'Demo fallback'}</strong>
            <span>{connection === 'live' ? (collectorSource === 'tauri-native' ? 'embedded · local · 1 sec' : 'loopback · local · 1 sec') : 'simulated telemetry'}</span>
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
