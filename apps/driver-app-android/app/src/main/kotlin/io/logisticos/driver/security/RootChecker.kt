package io.logisticos.driver.security

import android.content.Context
import androidx.annotation.VisibleForTesting
import com.scottyab.rootbeer.RootBeer
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject

class RootChecker @Inject constructor(
    @ApplicationContext private val context: Context?
) {
    @VisibleForTesting
    internal constructor(isRooted: Boolean) : this(null) {
        this._isRootedOverride = isRooted
    }

    private var _isRootedOverride: Boolean? = null

    fun check(): Boolean {
        _isRootedOverride?.let { return it }
        val ctx = context ?: return false
        return try {
            RootBeer(ctx).isRooted
        } catch (e: Exception) {
            false
        }
    }
}
