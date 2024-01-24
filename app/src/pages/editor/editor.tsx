import { Button } from "@/components/ui/button";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  CardFooter,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { match } from "ts-pattern";
import { useState } from "react";
import { motion } from "framer-motion";
import { Terms } from "./terms";

type Tab = "terms" | "value-sets" | "values";

export function Editor() {
  const [activeTab, setActiveTab] = useState<Tab>("terms");

  return (
    <main className="flex flex-col items-center gap-8">
      <Tabs
        className="w-[500px]"
        value={activeTab}
        onValueChange={(value) => setActiveTab(value as Tab)}
      >
        <TabsList className="grid w-full grid-cols-3">
          <TabsTrigger value="terms">Terms</TabsTrigger>
          <TabsTrigger value="value-sets">Value sets</TabsTrigger>
          <TabsTrigger value="values">Values</TabsTrigger>
        </TabsList>
      </Tabs>
      <div className="flex w-full">
        {match(activeTab)
          .with("terms", () => (
            <FadeIn key="terms">
              <Terms />
            </FadeIn>
          ))
          .with("value-sets", () => (
            <FadeIn key="value-sets">
              <ValueSets />
            </FadeIn>
          ))
          .with("values", () => (
            <FadeIn key="values">
              <Values />
            </FadeIn>
          ))
          .exhaustive()}
      </div>
    </main>
  );
}

function ValueSets() {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Password</CardTitle>
        <CardDescription>
          Change your password here. After saving, you'll be logged out.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-2">
        <div className="space-y-1">
          <Label htmlFor="current">Current password</Label>
          <Input id="current" type="password" />
        </div>
        <div className="space-y-1">
          <Label htmlFor="new">New password</Label>
          <Input id="new" type="password" />
        </div>
      </CardContent>
      <CardFooter>
        <Button>Save password</Button>
      </CardFooter>
    </Card>
  );
}

function Values() {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Notifications</CardTitle>
        <CardDescription>
          Change your notification settings here.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-2">
        <div className="space-y-1">
          <Label htmlFor="email">Email</Label>
          <Input id="email" defaultValue="" />
        </div>
        <div className="space-y-1">
          <Label htmlFor="push">Push</Label>
          <Input id="push" defaultValue="" />
        </div>
      </CardContent>
      <CardFooter>
        <Button>Save notifications</Button>
      </CardFooter>
    </Card>
  );
}

function FadeIn({ children }: { children: React.ReactNode }) {
  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration: 0.5 }}
      className="flex w-full"
    >
      {children}
    </motion.div>
  );
}
