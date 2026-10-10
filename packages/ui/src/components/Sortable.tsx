// Reordering a list: a pointer or a finger drags an item by its handle (@dnd-kit's sortable
// preset), and ArrowUp or ArrowDown on the focused handle moves it one place. The list says where
// an item went (`onMove(id, over)`: the item now stands where `over` stood); the caller keeps the
// order. After a keyboard move the handle keeps the focus, though the row moved in the page.
import {
  DndContext,
  type DragEndEvent,
  type Modifier,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import { SortableContext, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS as DndCss } from "@dnd-kit/utilities";
import {
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
} from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { Icon } from "./Icon";

interface ListState {
  ids: readonly string[];
  step: (id: string, by: -1 | 1) => void;
}

const ListContext = createContext<ListState | null>(null);

/** Items move up and down only. */
const vertical: Modifier = ({ transform }) => ({ ...transform, x: 0 });

/** An item's name, for what screen readers hear while it moves. */
function sortName(id: string | number): string {
  return (
    [...document.querySelectorAll<HTMLElement>("[data-sort-name]")].find(
      (el) => el.dataset.sortName === String(id),
    )?.dataset.sortLabel ?? ""
  );
}

export interface SortableListProps {
  /** The items' ids, in their order. */
  ids: readonly string[];
  /** `id` was dropped where `over` stands (or moved one place onto it). */
  onMove: (id: string, over: string) => void;
  children: ReactNode;
}

export function SortableList({ ids, onMove, children }: SortableListProps) {
  const t = useT();
  // A few pixels before a drag starts: a tap on the handle stays a tap.
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 4 } }));
  // The item moved by the keyboard, and the order it moved from.
  const focusAfter = useRef<{ id: string; from: string } | null>(null);
  const step = useCallback(
    (id: string, by: -1 | 1) => {
      const over = ids[ids.indexOf(id) + by];
      if (over === undefined) return;
      focusAfter.current = { id, from: ids.join("\n") };
      onMove(id, over);
    },
    [ids, onMove],
  );
  useEffect(() => {
    const moved = focusAfter.current;
    // Once the list stands in its new order: moving the row may have taken the focus away.
    if (moved === null || ids.join("\n") === moved.from) return;
    focusAfter.current = null;
    const handle = [...document.querySelectorAll<HTMLElement>("[data-sort-handle]")].find(
      (el) => el.dataset.sortHandle === moved.id,
    );
    handle?.focus();
  }, [ids]);
  const value = useMemo(() => ({ ids, step }), [ids, step]);
  const onDragEnd = (event: DragEndEvent) => {
    const over = event.over?.id;
    if (over !== undefined && over !== event.active.id)
      onMove(String(event.active.id), String(over));
  };
  return (
    <ListContext.Provider value={value}>
      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        modifiers={[vertical]}
        onDragEnd={onDragEnd}
        accessibility={{
          screenReaderInstructions: { draggable: t("ui.sortable.instructions") },
          announcements: {
            onDragStart: ({ active }) => t("ui.sortable.picked", { name: sortName(active.id) }),
            onDragOver: ({ over }) =>
              over ? t("ui.sortable.over", { name: sortName(over.id) }) : undefined,
            onDragEnd: ({ over }) =>
              over ? t("ui.sortable.dropped", { name: sortName(over.id) }) : undefined,
            onDragCancel: () => t("ui.sortable.cancelled"),
          },
        }}>
        <SortableContext items={[...ids]} strategy={verticalListSortingStrategy}>
          {children}
        </SortableContext>
      </DndContext>
    </ListContext.Provider>
  );
}

/** What an item's handle needs: spread `props` on it. */
export interface SortHandle {
  props: Record<string, unknown>;
  dragging: boolean;
}

export interface SortableItemProps {
  id: string;
  /** The item's name, for the handle's label and what screen readers hear while it moves. */
  label: string;
  className?: string;
  children: (handle: SortHandle) => ReactNode;
}

/** One item of a `SortableList`: it follows the pointer while dragged, and `children` gets its
 *  handle. */
export function SortableItem({ id, label, className, children }: SortableItemProps) {
  const t = useT();
  const list = useContext(ListContext);
  const {
    attributes,
    listeners,
    setNodeRef,
    setActivatorNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id, attributes: { roleDescription: t("ui.sortable.role") } });
  const style: CSSProperties = {
    transform: DndCss.Translate.toString(transform),
    transition,
    position: "relative",
    zIndex: isDragging ? 20 : undefined,
  };
  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    event.preventDefault();
    event.stopPropagation();
    list?.step(id, event.key === "ArrowUp" ? -1 : 1);
  };
  return (
    <div
      ref={setNodeRef}
      style={style}
      data-sort-name={id}
      data-sort-label={label}
      data-dragging={isDragging ? "" : undefined}
      className={cx(isDragging && "opacity-90 shadow-pop", className)}>
      {children({
        dragging: isDragging,
        props: {
          ...attributes,
          ...listeners,
          ref: setActivatorNodeRef,
          onKeyDown,
          "data-sort-handle": id,
          "aria-label": t("ui.sortable.handle", { name: label }),
        },
      })}
    </div>
  );
}

/** The grip an item is dragged by. `size` 44 is the phone's touch target. */
export function DragHandle({
  handle,
  size = 28,
  className,
}: {
  handle: SortHandle;
  size?: 28 | 44;
  className?: string;
}) {
  return (
    <button
      type="button"
      {...handle.props}
      className={cx(
        "inline-flex shrink-0 cursor-grab touch-none items-center justify-center rounded-6 text-fg-subtle transition-colors outline-none select-none hover:bg-inset hover:text-fg focus-visible:bg-inset focus-visible:text-fg active:cursor-grabbing",
        size === 44 ? "size-11" : "size-7",
        handle.dragging && "cursor-grabbing text-fg",
        className,
      )}>
      <Icon name="drag" size={size === 44 ? 20 : 16} />
    </button>
  );
}
