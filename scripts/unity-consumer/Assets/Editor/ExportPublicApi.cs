using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using Motu.Islands;
using UnityEngine;

public static class ExportPublicApi
{
    public static void Run()
    {
        var output = Environment.GetEnvironmentVariable("MOTU_API_OUTPUT");
        if (string.IsNullOrWhiteSpace(output)) throw new ArgumentException("Set MOTU_API_OUTPUT to the API report path.");
        var lines = new List<string> { "Declared public Motu API (inherited Unity members excluded)." };
        var assemblies = AppDomain.CurrentDomain.GetAssemblies()
            .Where(a => a.GetName().Name == typeof(IslandGenerator).Assembly.GetName().Name
                || a.GetName().Name == "Motu.Navigation")
            .OrderBy(a => a.GetName().Name);
        foreach (var assembly in assemblies)
        {
            lines.Add("\nAssembly: " + assembly.GetName().Name);
            foreach (var type in assembly.GetExportedTypes().OrderBy(t => t.FullName))
            {
                lines.Add("\n" + type.FullName);
                foreach (var member in type.GetMembers(BindingFlags.Public | BindingFlags.Instance | BindingFlags.Static | BindingFlags.DeclaredOnly)
                    .Where(m => !(m is MethodInfo method) || !method.IsSpecialName).OrderBy(m => m.ToString()))
                    lines.Add("  " + member);
            }
        }
        File.WriteAllLines(output, lines);
        Debug.Log("MOTU PUBLIC API EXPORTED: " + lines.Count + " entries");
    }
}
