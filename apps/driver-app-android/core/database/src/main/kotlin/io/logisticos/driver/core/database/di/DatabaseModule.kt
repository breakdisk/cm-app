package io.logisticos.driver.core.database.di

import android.content.Context
import androidx.room.Room
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import io.logisticos.driver.core.database.DriverDatabase
import io.logisticos.driver.core.database.MIGRATION_3_4
import io.logisticos.driver.core.database.MIGRATION_4_5
import io.logisticos.driver.core.database.MIGRATION_5_6
import io.logisticos.driver.core.database.MIGRATION_6_7
import io.logisticos.driver.core.database.MIGRATION_7_8
import io.logisticos.driver.core.database.MIGRATION_8_9
import io.logisticos.driver.core.database.dao.*
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object DatabaseModule {

    /**
     * Note on the destructive-fallback policy.
     *
     * This database is the durable buffer for offline chain-of-custody evidence —
     * the sync queue, unsent PODs and their photo paths, POP records. A blanket
     * `fallbackToDestructiveMigration()` means any future version bump shipped
     * without a matching migration silently deletes that evidence on the driver's
     * phone, with no error and no way to recover it.
     *
     * So the fallback is scoped to versions 1 and 2 only — the legacy schemas that
     * predate `MIGRATION_3_4` and genuinely have no upgrade path. Every version
     * from 3 onward must have a real migration; a missing one now fails loudly at
     * open time (and in the migration tests) instead of quietly wiping data.
     */
    @Provides @Singleton
    fun provideDatabase(@ApplicationContext context: Context): DriverDatabase =
        Room.databaseBuilder(context, DriverDatabase::class.java, "driver_app.db")
            .addMigrations(
                MIGRATION_3_4, MIGRATION_4_5, MIGRATION_5_6, MIGRATION_6_7, MIGRATION_7_8,
                MIGRATION_8_9,
            )
            .fallbackToDestructiveMigrationFrom(1, 2)
            .build()

    @Provides fun provideShiftDao(db: DriverDatabase): ShiftDao = db.shiftDao()
    @Provides fun provideTaskDao(db: DriverDatabase): TaskDao = db.taskDao()
    @Provides fun provideRouteDao(db: DriverDatabase): RouteDao = db.routeDao()
    @Provides fun providePodDao(db: DriverDatabase): PodDao = db.podDao()
    @Provides fun provideLocationBreadcrumbDao(db: DriverDatabase): LocationBreadcrumbDao = db.locationBreadcrumbDao()
    @Provides fun provideScanEventDao(db: DriverDatabase): ScanEventDao = db.scanEventDao()
    @Provides fun provideSyncQueueDao(db: DriverDatabase): SyncQueueDao = db.syncQueueDao()
    @Provides fun provideNotificationDao(db: DriverDatabase): NotificationDao = db.notificationDao()
}
