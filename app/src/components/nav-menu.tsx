import { micah } from "@dicebear/collection";
import { createAvatar } from "@dicebear/core";
import { Link, useMatchRoute } from "@tanstack/react-router";
import { motion } from "framer-motion";
import { useMemo } from "react";
import { Avatar, AvatarFallback, AvatarImage } from "./ui/avatar";
import { LogOut, Settings, User } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ThemeToggle } from "./theme-toggle";

export function NavMenu() {
  const avatar = useMemo(() => {
    return createAvatar(micah, {
      size: 128,
      seed: "Pontus",
      // ... other options
    }).toDataUriSync();
  }, []);

  return (
    <nav className="flex w-full flex-row justify-between border-b bg-card px-8">
      <div className="flex h-12 flex-row gap-4">
        <NavMenuLink to="/">Home</NavMenuLink>
        <NavMenuLink to="/editor">Editor</NavMenuLink>
        <NavMenuLink to="/about">About</NavMenuLink>
      </div>
      <div className="flew-row flex items-center gap-4">
        <ThemeToggle />
        <AvatarDropdownMenu avatar={avatar} />
      </div>
    </nav>
  );
}

function NavMenuLink({
  to,
  children,
}: {
  to: string;
  children: React.ReactNode;
}) {
  const matchRoute = useMatchRoute();

  const isActive = matchRoute({ to });

  const variants = {
    active: {
      color: "#16A349",
      boxShadow: "0 4px 0 -2px rgba(22,163,73,1)",
    },
    inactive: { color: "#6b7280", boxShadow: "none" },
  };

  return (
    <motion.div
      animate={isActive ? "active" : "inactive"}
      variants={variants}
      className="flex items-center"
    >
      <Link
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        to={to as any}
        className="text-base font-semibold transition-colors hover:text-primary/80"
      >
        {children}
      </Link>
    </motion.div>
  );
}

export function AvatarDropdownMenu(props: { avatar: string }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Avatar className="rounded-full border-2 bg-primary/70 transition-colors hover:cursor-pointer hover:border-primary">
          <AvatarImage src={props.avatar} />
          <AvatarFallback>CN</AvatarFallback>
        </Avatar>
      </DropdownMenuTrigger>
      <DropdownMenuContent className="w-56">
        <DropdownMenuLabel>My Account</DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuItem>
            <User className="mr-2 h-4 w-4" />
            <span>Profile</span>
            <DropdownMenuShortcut>⌘P</DropdownMenuShortcut>
          </DropdownMenuItem>
          <DropdownMenuItem>
            <Settings className="mr-2 h-4 w-4" />
            <span>Settings</span>
            <DropdownMenuShortcut>⌘S</DropdownMenuShortcut>
          </DropdownMenuItem>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          onClick={() => {
            localStorage.removeItem("isAuthenticated");
            window.location.reload();
          }}
        >
          <LogOut className="mr-2 h-4 w-4" />
          <span>Log out</span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
