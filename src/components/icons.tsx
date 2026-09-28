import {
  Activity01Icon,
  AlertCircleIcon,
  ArrowDown01Icon,
  ArrowLeftRightIcon,
  ArrowUp01Icon,
  ArrowUpDownIcon,
  ArrowUpRight01Icon,
  BadgeDollarSignIcon,
  BanIcon,
  BanknoteIcon,
  BedDoubleIcon,
  BriefcaseBusinessIcon,
  CalendarDaysIcon,
  CarFrontIcon,
  ChartCandlestickIcon,
  ChartNoAxesCombinedIcon,
  CheckIcon,
  ChevronDownIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  CircleEllipsisIcon,
  ClapperboardIcon,
  Coffee01Icon,
  Coins01Icon,
  CpuIcon,
  CreditCardIcon,
  DatabaseIcon,
  DollarSignIcon,
  Download01Icon,
  EllipsisIcon,
  EyeIcon,
  EyeOffIcon,
  FilePlusIcon,
  FilterIcon,
  FingerPrintIcon,
  FuelIcon,
  GlobeIcon,
  GraduationCapIcon,
  HandCoinsIcon,
  HardDriveIcon,
  HeartPulseIcon,
  HelpCircleIcon,
  HistoryIcon,
  House02Icon,
  InfoIcon,
  LandmarkIcon,
  LinkIcon,
  ListIcon,
  MapPinnedIcon,
  MessageCircleIcon,
  MonitorPlayIcon,
  PackageIcon,
  PaletteIcon,
  PercentIcon,
  PlaneIcon,
  ReceiptIcon,
  ReceiptTextIcon,
  RefreshCwIcon,
  RepeatIcon,
  RotateCcwIcon,
  SaveIcon,
  ScrollTextIcon,
  Search01Icon,
  Settings01Icon,
  ShapesIcon,
  ShieldCheckIcon,
  ShoppingBag01Icon,
  ShoppingBasket01Icon,
  SparklesIcon,
  SproutIcon,
  Store01Icon,
  Tag01Icon,
  TrashIcon,
  TrendingUpIcon,
  UnfoldMoreIcon,
  Unlink01Icon,
  UtensilsCrossedIcon,
  WalletCardsIcon,
  Wrench01Icon,
  XIcon,
  ZapIcon,
} from '@hugeicons/core-free-icons'
import { HugeiconsIcon, type HugeiconsIconProps, type IconSvgElement } from '@hugeicons/react'

function createIcon(icon: IconSvgElement) {
  return function Icon(props: Omit<HugeiconsIconProps, 'icon'>) {
    return (
      <HugeiconsIcon
        icon={icon}
        strokeWidth={1.75}
        aria-hidden="true"
        {...props}
        className={props.className ? `brief-icon ${props.className}` : 'brief-icon'}
      />
    )
  }
}

export type IconComponent = ReturnType<typeof createIcon>

export const Activity = createIcon(Activity01Icon)
export const AlertCircle = createIcon(AlertCircleIcon)
export const ArrowDown = createIcon(ArrowDown01Icon)
export const ArrowDownUp = createIcon(ArrowUpDownIcon)
export const ArrowLeftRight = createIcon(ArrowLeftRightIcon)
export const ArrowUp = createIcon(ArrowUp01Icon)
export const ArrowUpRight = createIcon(ArrowUpRight01Icon)
export const BadgeDollarSign = createIcon(BadgeDollarSignIcon)
export const Ban = createIcon(BanIcon)
export const Banknote = createIcon(BanknoteIcon)
export const BedDouble = createIcon(BedDoubleIcon)
export const BriefcaseBusiness = createIcon(BriefcaseBusinessIcon)
export const CalendarDays = createIcon(CalendarDaysIcon)
export const CarFront = createIcon(CarFrontIcon)
export const ChartCandlestick = createIcon(ChartCandlestickIcon)
export const ChartNoAxesCombined = createIcon(ChartNoAxesCombinedIcon)
export const Check = createIcon(CheckIcon)
export const ChevronDown = createIcon(ChevronDownIcon)
export const ChevronLeft = createIcon(ChevronLeftIcon)
export const ChevronRight = createIcon(ChevronRightIcon)
export const ChevronsUpDown = createIcon(UnfoldMoreIcon)
export const CircleEllipsis = createIcon(CircleEllipsisIcon)
export const Clapperboard = createIcon(ClapperboardIcon)
export const Coffee = createIcon(Coffee01Icon)
export const Coins = createIcon(Coins01Icon)
export const Cpu = createIcon(CpuIcon)
export const CreditCard = createIcon(CreditCardIcon)
export const Database = createIcon(DatabaseIcon)
export const DollarSign = createIcon(DollarSignIcon)
export const Download = createIcon(Download01Icon)
export const Ellipsis = createIcon(EllipsisIcon)
export const Eye = createIcon(EyeIcon)
export const EyeOff = createIcon(EyeOffIcon)
export const FilePlus = createIcon(FilePlusIcon)
export const Filter = createIcon(FilterIcon)
export const Fingerprint = createIcon(FingerPrintIcon)
export const Fuel = createIcon(FuelIcon)
export const Globe2 = createIcon(GlobeIcon)
export const GraduationCap = createIcon(GraduationCapIcon)
export const HandCoins = createIcon(HandCoinsIcon)
export const HardDrive = createIcon(HardDriveIcon)
export const HeartPulse = createIcon(HeartPulseIcon)
export const HelpCircle = createIcon(HelpCircleIcon)
export const History = createIcon(HistoryIcon)
export const House = createIcon(House02Icon)
export const Info = createIcon(InfoIcon)
export const Landmark = createIcon(LandmarkIcon)
export const Link2 = createIcon(LinkIcon)
export const List = createIcon(ListIcon)
export const MapPinned = createIcon(MapPinnedIcon)
export const MessageCircle = createIcon(MessageCircleIcon)
export const MonitorPlay = createIcon(MonitorPlayIcon)
export const Package = createIcon(PackageIcon)
export const Palette = createIcon(PaletteIcon)
export const Percent = createIcon(PercentIcon)
export const Plane = createIcon(PlaneIcon)
export const Receipt = createIcon(ReceiptIcon)
export const ReceiptText = createIcon(ReceiptTextIcon)
export const RefreshCw = createIcon(RefreshCwIcon)
export const Repeat2 = createIcon(RepeatIcon)
export const RotateCcw = createIcon(RotateCcwIcon)
export const Save = createIcon(SaveIcon)
export const ScrollText = createIcon(ScrollTextIcon)
export const Search = createIcon(Search01Icon)
export const Settings = createIcon(Settings01Icon)
export const Shapes = createIcon(ShapesIcon)
export const ShieldCheck = createIcon(ShieldCheckIcon)
export const ShoppingBag = createIcon(ShoppingBag01Icon)
export const ShoppingBasket = createIcon(ShoppingBasket01Icon)
export const Sparkles = createIcon(SparklesIcon)
export const Sprout = createIcon(SproutIcon)
export const Store = createIcon(Store01Icon)
export const Tag = createIcon(Tag01Icon)
export const Trash2 = createIcon(TrashIcon)
export const TrendingUp = createIcon(TrendingUpIcon)
export const Unlink = createIcon(Unlink01Icon)
export const UtensilsCrossed = createIcon(UtensilsCrossedIcon)
export const WalletCards = createIcon(WalletCardsIcon)
export const Wrench = createIcon(Wrench01Icon)
export const X = createIcon(XIcon)
export const Zap = createIcon(ZapIcon)
