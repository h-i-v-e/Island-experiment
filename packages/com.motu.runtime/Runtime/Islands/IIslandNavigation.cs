using System;
using System.Threading.Tasks;

namespace Motu.Islands
{
    /// <summary>Optional navigation lifetime. Concrete agent APIs live in com.motu.navigation.</summary>
    public interface IIslandNavigation : IDisposable
    {
        bool IsReady { get; }
        bool IsRegistered { get; }
        Task BuildCompletion { get; }
        void SetRegistered(bool registered);

        /// <summary>Distance from the island origin, in metres. A world island
        /// bakes only after the viewer comes within approach range.</summary>
        void NoteViewerDistance(float distanceMetres);
    }
}
